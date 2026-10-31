# Control protocol and ownership

This document explains the browser, Rust worker, and Lua bridge contract.
The messages describe button state, not commands to execute arbitrary Lua.
The protocol is local to the PC and is not authentication.

## Browser to Rust

`POST /controls` carries JSON with integer `session`, `revision`, `left`, and `right` fields.
Each mask has 20 bits. Bit zero represents pushbutton 1; bit 19 represents pushbutton 20.
The server accepts only the current viewer's IP and session, increasing nonzero revisions, and masks no larger than 1048575.
HTTP 202 means queued, not that DCS has acknowledged the action.
The queue holds at most 64 snapshots.

A browser connection generation owns its status requests and control pump.
Disconnect invalidates that generation and aborts pending requests.
Replies from an older generation cannot change the current viewer's state.
Status polling is single-flight and times out after five seconds.
A control request times out after 1.5 seconds.

Held inputs publish snapshots every 150 milliseconds.
Pointer cancellation, hiding the page, losing focus, changing views, and disconnect release held inputs.
A release beacon can outlive the page; the server's revision check rejects an earlier request that arrives after that release.
Reconnecting starts a new control session and resets browser revisions.

## Rust to Lua

The Lua bridge listens on `127.0.0.1:17881` inside the dedicated DCS.Tablet profile.
The Rust worker owns one connected loopback UDP socket and polls every 20 milliseconds.

```text
Idle probe:      HDDI1 PING
Idle reply:      HDDI1 PONG READY
                HDDI1 PONG WAIT
State snapshot: HDDI1 <epoch> <serial> <left-mask> <right-mask>
Acknowledgment: HDDI1 <epoch> <serial> READY
                HDDI1 <epoch> <serial> WAIT
```

READY means the current aircraft is the supported F/A-18C; WAIT means it is not.
The worker epoch identifies a worker lifetime and is independent of the browser session.
The serial advances for changed snapshots and session-release handshakes.
Rust retries the current snapshot until its matching acknowledgement arrives.
Lua acknowledges duplicate serials without applying the same press again.
Old serials are ignored.
A different epoch or source port must first send both masks released before becoming owner.

Rust drains up to 256 pending replies per polling cycle.
This prevents stale duplicate replies from delaying a new acknowledgement after DCS resumes.
Lua reads at most 32 packets per frame so networking cannot monopolize the simulation callback.

## Release limits

Rust considers acknowledgements fresh for 700 milliseconds.
A stale acknowledgement disables new input independently of the video connection.
Lua releases held buttons after 750 milliseconds without a valid state packet.
Lua also ends a continuous hold after two seconds, even with fresh heartbeats.
An expired hold is blocked until a released mask rearms that button.
Lua cleanup runs on aircraft changes and export shutdown, and preserves prior export callbacks.

These timers serve different purposes; increasing the browser HTTP timeout does not extend the Lua hold limit.
A DCS simulation stall can prevent callbacks and temporarily interrupt readiness.
The watchdogs favor releasing controls over preserving a stuck press.
