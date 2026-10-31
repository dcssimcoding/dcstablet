# Third-party notices

The source repository excludes user settings, downloaded drivers, historical logs, and application binaries.

## Software components

Each component keeps its own license and copyright notices.

| Component | Source |
| --- | --- |
| windows-capture | [Source repository](https://github.com/NiiightmareXD/windows-capture) |
| str0m | [Source repository](https://github.com/algesten/str0m) |
| httparse | [Source repository](https://github.com/seanmonstar/httparse) |
| serde_json | [Source repository](https://github.com/serde-rs/json) |
| windows Rust bindings | [Source repository](https://github.com/microsoft/windows-rs) |

`Cargo.lock` identifies the complete dependency versions.
The package script collects their source license files into `third-party-licenses`.
That folder includes an index.
The script stops if it cannot find license text for a dependency.

Keep the license folder and this notice with a distributed Windows package.

## Supplemental license information

The `licenses` source folder supplies notices for crates that omit separate license files.
Its source notes identify upstream commits.
## Windows media components

DCS Tablet 0.1.0 calls the Microsoft H.264 encoder supplied by Windows Media Foundation.
It does not include OpenH264, a separately compiled H.264 library, or a Microsoft codec DLL.
Microsoft supplies and licenses the Windows components separately from this project's MIT license.
The applicable Windows license terms govern those components and their permitted uses.
This project does not grant additional codec patent rights or claim that every use is patent-cleared.

[Microsoft's encoder documentation](https://learn.microsoft.com/en-us/windows/win32/medfound/h-264-video-encoder) describes the Windows component.

## Documentation tools

Python-Markdown is a build-time documentation tool under the BSD-3-Clause license.
[Its upstream project](https://github.com/Python-Markdown/markdown) supplies its license and source.
The Windows release contains the generated guide, not the Python-Markdown implementation.

## Separate products

The user installs the optional [Virtual Display Driver](https://github.com/VirtualDrivers/Virtual-Display-Driver) separately.
Its own license and installation conditions apply.
DCS Tablet does not distribute that driver.

DCS World and related product names belong to their respective owners.
DCS Tablet does not distribute aircraft assets, DCS binaries, or tablet browser binaries.
The project has no affiliation with Eagle Dynamics.
