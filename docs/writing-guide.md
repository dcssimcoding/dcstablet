# Documentation writing guide

Use ASD-STE100 Simplified Technical English, Issue 9, for this documentation.
The [official specification](https://www.asd-ste100.org/assets/files/ASD-STE100_ISSUE9.pdf) gives the writing rules and dictionary.

## Writing rules

1. Give one instruction in each procedural sentence.
2. Keep procedural sentences within 20 words.
3. Keep descriptive sentences within 25 words.
4. Use the active voice.
5. Put a necessary condition before its instruction.
6. Use the same term for the same item.
7. Define an abbreviation before unfamiliar use.
8. Give a result after a task.
9. Use notes only for information.
10. Check each word's meaning and part of speech against the dictionary.

Sentence length alone does not establish STE conformity.
A reviewer must also check word meanings, technical terms, grammar, and task accuracy.
The project does not claim independent STE certification.

## Technical vocabulary

The [glossary](glossary.md) defines the user terms.
Product names, file names, command names, code identifiers, and exact interface labels retain their required spelling.

Computer operations use these technical verbs when applicable:

| Verb | Specific use |
| --- | --- |
| Click | Select a PC control with the mouse. |
| Double-click | Open a file with two mouse clicks. |
| Right-click | Open a PC context menu. |
| Tap | Select a tablet control by touch. |
| Enter | Put text in a field or command window. |
| Paste | Insert copied text. |
| Copy | Put a duplicate of selected file data or text in the Windows clipboard. |
| Save | Write changed file data to storage. |
| Open | Show a file, folder, or application. |
| Close | End a file, folder, or application session. |
| Minimize | Hide an application window. Keep its process active. |
| Activate | Make a virtual display available to Windows. |
| Restore | Return a saved software setting or display state. |
| Download | Transfer a release file to the PC. |
| Extract | Copy files out of a ZIP package. |
| Install | Add software to the PC. |
| Encrypt | Protect data with cryptography. |

These terms apply to software operations only.
Do not use them as general alternatives to approved words.

Technical reference pages also use protocol and software terms from their source interfaces.
Examples include HTTP, UDP, Rust, JSON, WebRTC, and Windows Graphics Capture.

## Examples

Less clear:

> Launch the streamer and subsequently navigate to its endpoint using your preferred client.

Preferred:

1. Double-click `Start-Demo.cmd`.
2. Open the address on the tablet.
3. Tap **Connect**.

Less clear:

> Ensure your viewport is configured correctly before initiating the session.

Preferred:

1. Set the virtual display to **1024 × 512**.
2. Start DCS Tablet.

## Review checklist

1. Follow the task as a new user.
2. Check every command against the application.
3. Check links and image paths.
4. Check sentence lengths and terms.
5. Check that examples identify values that the user must replace.
6. Keep license texts unchanged.
7. Update the browser-readable guide after changes to user instructions.

The canonical user text is in `README.md` and the user documentation files in `docs`.
`START-HERE.html` contains their browser-readable copy.
