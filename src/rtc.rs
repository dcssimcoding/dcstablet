//! One viewer owns one UDP/WebRTC session and its video pipeline. The event loop
//! pumps network input/output; dropping the pipeline cancels and joins its workers.
//! Encoded timestamps use a 90-kHz RTP clock relative to this session start.
use crate::{
    STOP,
    config::Config,
    encode::Pipeline,
    faults,
    state::{Stage, State},
};
use serde_json::Value;
use std::{
    io::ErrorKind,
    net::{IpAddr, SocketAddr, UdpSocket},
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
use str0m::{
    Candidate, Event, IceConnectionState, Input, Output, Rtc, RtcConfig,
    change::SdpOffer,
    format::Codec,
    media::{MediaKind, MediaTime, Mid},
    net::{Protocol, Receive},
};

pub struct Session {
    rtc: Rtc,
    socket: UdpSocket,
}
pub fn prepare(offer: Value, remote: IpAddr) -> Result<(Session, Value), String> {
    let sdp = offer["sdp"].as_str().ok_or("Expected SDP text")?;
    if offer["type"] != "offer"
        || sdp
            .lines()
            .filter(|line| line.starts_with("m=video "))
            .count()
            != 1
        || sdp
            .lines()
            .any(|line| line.starts_with("m=audio ") || line.starts_with("m=application "))
    {
        return Err("Expected one video-only SDP offer".into());
    }
    let offer: SdpOffer = serde_json::from_value(offer).map_err(|e| e.to_string())?;
    let route = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    let route_target = if remote.is_loopback() {
        "192.0.2.1".parse().unwrap()
    } else {
        remote
    };
    route
        .connect(SocketAddr::new(route_target, 9))
        .map_err(|e| e.to_string())?;
    let local_ip = route.local_addr().map_err(|e| e.to_string())?.ip();
    let socket = UdpSocket::bind(SocketAddr::new(local_ip, 0)).map_err(|e| e.to_string())?;
    let mut rtc = RtcConfig::new()
        .clear_codecs()
        .enable_h264(true)
        .build(Instant::now());
    rtc.add_local_candidate(
        Candidate::host(socket.local_addr().map_err(|e| e.to_string())?, "udp")
            .map_err(|e| e.to_string())?,
    )
    .ok_or("Could not add local ICE candidate")?;
    let answer = rtc
        .sdp_api()
        .accept_offer(offer)
        .map_err(|e| e.to_string())?;
    let answer = serde_json::to_value(answer).map_err(|e| e.to_string())?;
    if !answer["sdp"].as_str().unwrap_or("").contains("H264/90000") {
        return Err("Peer did not offer compatible H.264".into());
    }
    Ok((Session { rtc, socket }, answer))
}
pub fn run(mut session: Session, cfg: Arc<Config>, state: Arc<State>) -> Result<(), String> {
    let started = Instant::now();
    let local = session.socket.local_addr().map_err(|e| e.to_string())?;
    let mut mid: Option<Mid> = None;
    let mut pipeline: Option<Pipeline> = None;
    let mut sequence = 0;
    let mut last_receive = started;
    let mut buffer = [0u8; 65536];
    while !STOP.load(Ordering::Relaxed) && session.rtc.is_alive() {
        let timeout = loop {
            match session.rtc.poll_output().map_err(|e| e.to_string())? {
                Output::Timeout(deadline) => break deadline,
                Output::Transmit(transmit) => {
                    if let Err(error) = session
                        .socket
                        .send_to(&transmit.contents, transmit.destination)
                        && !matches!(
                            error.kind(),
                            ErrorKind::NetworkUnreachable
                                | ErrorKind::HostUnreachable
                                | ErrorKind::AddrNotAvailable
                        )
                    {
                        return Err(error.to_string());
                    }
                }
                Output::Event(event) => match event {
                    Event::MediaAdded(media) if media.kind == MediaKind::Video => {
                        mid = Some(media.mid);
                    }
                    Event::Connected if pipeline.is_none() => {
                        pipeline = Some(Pipeline::start(cfg.clone(), state.clone()).inspect_err(
                            |error| {
                                state.fail(Stage::Capture, error);
                            },
                        )?);
                    }
                    Event::KeyframeRequest(_) => {
                        if let Some(pipe) = &pipeline {
                            pipe.keyframe.store(true, Ordering::Relaxed);
                        }
                    }
                    Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                        return Ok(());
                    }
                    _ => {}
                },
            }
        };
        if faults::take(&cfg, "sender-stall") {
            std::thread::sleep(Duration::from_millis(500));
        }
        if let Some(pipe) = &mut pipeline {
            pipe.check_health()?;
            if last_receive.elapsed() > Duration::from_secs(15) {
                return Ok(());
            }
            if let (Some(mid), Some(frame)) = (mid, pipe.encoded.take())
                && frame.sequence != sequence
            {
                if sequence > 0 && frame.sequence > sequence + 1 {
                    state
                        .skipped_encoded
                        .fetch_add(frame.sequence - sequence - 1, Ordering::Relaxed);
                }
                sequence = frame.sequence;
                let Some(writer) = session.rtc.writer(mid) else {
                    return Ok(());
                };
                let pt = writer
                    .payload_params()
                    .find(|p| p.spec().codec == Codec::H264)
                    .map(|p| p.pt())
                    .ok_or("No negotiated H.264 payload")?;
                let clock = frame.at.duration_since(started).as_micros() as u64 * 90 / 1000;
                writer
                    .write(
                        pt,
                        frame.at,
                        MediaTime::from_90khz(clock),
                        frame.bytes.clone(),
                    )
                    .map_err(|e| e.to_string())?;
                state.sent.fetch_add(1, Ordering::Relaxed);
                state.encode_to_send.add(frame.at.elapsed());
                state.source_to_send.add(frame.source_at.elapsed());
                continue;
            }
        } else if started.elapsed() > Duration::from_secs(15) {
            return Ok(());
        }
        let now = Instant::now();
        let delay = timeout
            .saturating_duration_since(now)
            .min(Duration::from_millis(5));
        if delay.is_zero() {
            session
                .rtc
                .handle_input(Input::Timeout(now))
                .map_err(|e| e.to_string())?;
            continue;
        }
        session
            .socket
            .set_read_timeout(Some(delay))
            .map_err(|e| e.to_string())?;
        match session.socket.recv_from(&mut buffer) {
            Ok((count, source)) => {
                let Ok(contents) = buffer[..count].try_into() else {
                    continue;
                };
                let input = Input::Receive(
                    Instant::now(),
                    Receive {
                        proto: Protocol::Udp,
                        source,
                        destination: local,
                        contents,
                    },
                );
                if session.rtc.accepts(&input) {
                    last_receive = Instant::now();
                    session.rtc.handle_input(input).map_err(|e| e.to_string())?;
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                session
                    .rtc
                    .handle_input(Input::Timeout(Instant::now()))
                    .map_err(|e| e.to_string())?;
            }
            Err(error) if error.kind() == ErrorKind::ConnectionReset => {
                eprintln!("Viewer transport ended: {error}");
                return Ok(());
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    session.rtc.disconnect();
    Ok(())
}
