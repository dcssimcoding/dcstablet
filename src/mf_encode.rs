//! Windows H.264 transform; no codec implementation is redistributed.
//! Encoder construction, calls and destruction stay on one MTA worker thread.
//! Samples use NV12 input and 100-nanosecond timestamps; output is Annex-B H.264.
use std::mem::ManuallyDrop;
use windows::{
    Win32::{
        Media::MediaFoundation::*,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            Variant::VARIANT,
        },
    },
    core::{Interface, Result},
};

struct Runtime;
impl Runtime {
    fn start() -> Result<Self> {
        // SAFETY: initialize this worker thread's COM apartment exactly once.
        // A successful initialization is balanced by Runtime::drop or failure cleanup.
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            if let Err(error) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                CoUninitialize();
                return Err(error);
            }
        }
        Ok(Self)
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        // SAFETY: all transform references are released before Runtime is dropped,
        // on the same thread that completed startup.
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}

// Created, used and dropped inside the encoder thread. COM interfaces are
// released before the final field shuts down Media Foundation and COM.
pub struct Encoder {
    transform: IMFTransform,
    codec: ICodecAPI,
    duration: i64,
    _runtime: Runtime,
}
impl Encoder {
    pub fn new(width: usize, height: usize, fps: u32, bitrate: u32) -> Result<Self> {
        let runtime = Runtime::start()?;
        // SAFETY: runtime keeps COM/MF active; all interfaces stay on this worker.
        unsafe {
            let transform: IMFTransform =
                CoCreateInstance(&CMSH264EncoderMFT, None, CLSCTX_INPROC_SERVER)?;
            let codec: ICodecAPI = transform.cast()?;
            codec.SetValue(&CODECAPI_AVLowLatencyMode, &VARIANT::from(true))?;
            codec.SetValue(
                &CODECAPI_AVEncCommonRateControlMode,
                &VARIANT::from(eAVEncCommonRateControlMode_CBR.0 as u32),
            )?;
            codec.SetValue(&CODECAPI_AVEncMPVGOPSize, &VARIANT::from(fps * 2))?;
            codec.SetValue(&CODECAPI_AVEncNumWorkerThreads, &VARIANT::from(1u32))?;

            let output = MFCreateMediaType()?;
            output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            output.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
            output.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Base.0 as u32)?;
            output.SetUINT32(&MF_MT_MPEG2_LEVEL, 31)?;
            video_shape(&output, width, height, fps)?;
            // The Microsoft H.264 MFT requires output before input.
            transform.SetOutputType(0, &output, 0)?;
            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
            video_shape(&input, width, height, fps)?;
            transform.SetInputType(0, &input, 0)?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
            Ok(Self {
                transform,
                codec,
                duration: 10_000_000 / i64::from(fps),
                _runtime: runtime,
            })
        }
    }

    pub fn encode(&mut self, nv12: &[u8], timestamp: i64, keyframe: bool) -> Result<Vec<u8>> {
        // SAFETY: input is copied while its MF buffer is locked; it has exactly
        // the allocation length requested below. No borrowed pointer escapes Unlock.
        unsafe {
            if keyframe {
                self.codec
                    .SetValue(&CODECAPI_AVEncVideoForceKeyFrame, &VARIANT::from(1u32))?;
            }
            let sample = MFCreateSample()?;
            let buffer = MFCreateMemoryBuffer(nv12.len() as u32)?;
            let mut destination = std::ptr::null_mut();
            buffer.Lock(&mut destination, None, None)?;
            // Lock provides at least the requested buffer length until Unlock.
            std::ptr::copy_nonoverlapping(nv12.as_ptr(), destination, nv12.len());
            buffer.Unlock()?;
            buffer.SetCurrentLength(nv12.len() as u32)?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(timestamp)?;
            sample.SetSampleDuration(self.duration)?;
            self.transform.ProcessInput(0, &sample, 0)?;
            self.output()
        }
    }

    fn output(&self) -> Result<Vec<u8>> {
        // SAFETY: output structures are initialized, owned COM references are taken
        // exactly once, and locked sample bytes are copied before Unlock.
        unsafe {
            // A synchronous transform may announce its sequence header on the
            // first output. Retry once after accepting that format change.
            for _ in 0..2 {
                let info = self.transform.GetOutputStreamInfo(0)?;
                let sample = if info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0 {
                    None
                } else {
                    let sample = MFCreateSample()?;
                    let buffer = MFCreateAlignedMemoryBuffer(
                        info.cbSize.max(1),
                        info.cbAlignment.saturating_sub(1),
                    )?;
                    sample.AddBuffer(&buffer)?;
                    Some(sample)
                };
                let mut output = MFT_OUTPUT_DATA_BUFFER {
                    pSample: ManuallyDrop::new(sample),
                    ..Default::default()
                };
                let mut status = 0;
                let result =
                    self.transform
                        .ProcessOutput(0, std::slice::from_mut(&mut output), &mut status);
                // The generated ABI structure uses ManuallyDrop. Release both
                // returned COM references on success and on every error path.
                let sample = ManuallyDrop::take(&mut output.pSample);
                drop(ManuallyDrop::take(&mut output.pEvents));
                match result {
                    Err(error) if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => {
                        return Ok(Vec::new());
                    }
                    Err(error) if error.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                        let format = self.transform.GetOutputAvailableType(0, 0)?;
                        self.transform.SetOutputType(0, &format, 0)?;
                        continue;
                    }
                    Err(error) => return Err(error),
                    Ok(()) => {}
                }
                let Some(sample) = sample else {
                    return Ok(Vec::new());
                };
                let buffer = sample.ConvertToContiguousBuffer()?;
                let mut source = std::ptr::null_mut();
                let mut length = 0;
                buffer.Lock(&mut source, None, Some(&mut length))?;
                let mut bytes = std::slice::from_raw_parts(source, length as usize).to_vec();
                buffer.Unlock()?;
                // WebRTC needs SPS/PPS when an IDR is requested. Some MFT
                // outputs put these only in MF_MT_MPEG_SEQUENCE_HEADER.
                if has_nal(&bytes, 5) && !(has_nal(&bytes, 7) && has_nal(&bytes, 8)) {
                    let format = self.transform.GetOutputCurrentType(0)?;
                    if let Ok(length) = format.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) {
                        let mut header = vec![0; length as usize];
                        format.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut header, None)?;
                        header.extend_from_slice(&bytes);
                        bytes = header;
                    }
                }
                return Ok(bytes);
            }
            Err(windows::core::Error::new(
                MF_E_TRANSFORM_STREAM_CHANGE,
                "Repeated encoder output format changes",
            ))
        }
    }
}
impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: the transform and Runtime are still live on their original thread.
        unsafe {
            let _ = self
                .transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
            let _ = self
                .transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
        }
    }
}

fn video_shape(media: &IMFMediaType, width: usize, height: usize, fps: u32) -> Result<()> {
    // SAFETY: media is a live owned COM interface; setters copy integer attributes.
    unsafe {
        media.SetUINT64(&MF_MT_FRAME_SIZE, ((width as u64) << 32) | height as u64)?;
        media.SetUINT64(&MF_MT_FRAME_RATE, (u64::from(fps) << 32) | 1)?;
        media.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1 << 32) | 1)?;
        media.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
    }
    Ok(())
}

pub(crate) fn has_nal(bytes: &[u8], kind: u8) -> bool {
    bytes
        .windows(4)
        .any(|w| w[..3] == [0, 0, 1] && w[3] & 31 == kind)
}

/// Top-down BGRA to limited-range BT.601 NV12, with chroma averaged over 2x2 pixels.
pub fn bgra_to_nv12(
    bgra: &[u8],
    width: usize,
    height: usize,
    output: &mut Vec<u8>,
) -> std::result::Result<(), String> {
    let pixels = width
        .checked_mul(height)
        .filter(|n| *n <= 4096 * 4096)
        .ok_or("Invalid pixel dimensions")?;
    if width == 0
        || height == 0
        || !width.is_multiple_of(2)
        || !height.is_multiple_of(2)
        || bgra.len() != pixels * 4
    {
        return Err("NV12 requires an even-sized, tightly packed BGRA image".into());
    }
    output.resize(pixels * 3 / 2, 0);
    for y in (0..height).step_by(2) {
        for x in (0..width).step_by(2) {
            let (mut sum_r, mut sum_g, mut sum_b) = (0, 0, 0);
            for dy in 0..2 {
                for dx in 0..2 {
                    let at = (y + dy) * width + x + dx;
                    let (b, g, r) = (
                        i32::from(bgra[at * 4]),
                        i32::from(bgra[at * 4 + 1]),
                        i32::from(bgra[at * 4 + 2]),
                    );
                    output[at] =
                        (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(16, 235) as u8;
                    sum_r += r;
                    sum_g += g;
                    sum_b += b;
                }
            }
            let (r, g, b) = ((sum_r + 2) / 4, (sum_g + 2) / 4, (sum_b + 2) / 4);
            let at = pixels + (y / 2) * width + x;
            output[at] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(16, 240) as u8;
            output[at + 1] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(16, 240) as u8;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nv12_black_white_and_chroma_average() {
        let mut output = Vec::new();
        bgra_to_nv12(&[0, 0, 0, 255].repeat(4), 2, 2, &mut output).unwrap();
        assert_eq!(output, [16, 16, 16, 16, 128, 128]);
        bgra_to_nv12(&[255, 255, 255, 255].repeat(4), 2, 2, &mut output).unwrap();
        assert_eq!(output, [235, 235, 235, 235, 128, 128]);
        assert!(bgra_to_nv12(&[0; 12], 1, 3, &mut output).is_err());
        assert!(bgra_to_nv12(&[0; 4], 2, 2, &mut output).is_err());
        let mixed = [
            0, 0, 255, 255, 255, 0, 0, 255, 0, 0, 255, 255, 255, 0, 0, 255,
        ];
        bgra_to_nv12(&mixed, 2, 2, &mut output).unwrap();
        assert_eq!(output, [82, 41, 82, 41, 165, 175]);
    }
    #[test]
    fn windows_encoder_emits_parameter_sets_and_requested_keyframes() {
        let mut encoder = Encoder::new(256, 128, 30, 1_000_000).unwrap();
        let mut frame = vec![128; 256 * 128 * 3 / 2];
        let first = encoder.encode(&frame, 0, true).unwrap();
        assert!(
            has_nal(&first, 5) && has_nal(&first, 7) && has_nal(&first, 8),
            "First frame must be a decodable IDR"
        );
        frame[0] = 32;
        let next = encoder.encode(&frame, 333_333, false).unwrap();
        assert!(
            !next.is_empty(),
            "Low-latency encoding must not buffer a frame"
        );
        let key = encoder.encode(&frame, 666_666, true).unwrap();
        assert!(
            has_nal(&key, 5) && has_nal(&key, 7) && has_nal(&key, 8),
            "Requested keyframe must include parameter sets"
        );
    }
}
