# Fantech Maxfit61 wired USB HID protocol — static-analysis specification

Analysis date: 2026-11-18. Target supplied by user: VID 0x0416, PID 0xC345.

## Evidence status and scope

**CONFIRMED** means observed in this exact vendor application's code or data, with preferred virtual addresses (VAs) below. It does **not** mean verified against a connected keyboard. **GUESSED** means interpretation, extrapolation to this keyboard, or an explicitly unresolved field. Unknown bytes are never assigned invented values. This is a partial protocol specification, not a complete implementation recipe.

Only static analysis was performed. Neither FANTECH.exe nor XS61BRGB_V10307.exe was executed. No HID library was loaded or called, no reports were sent, no software was installed or downloaded, and no files under Program Files were changed. The firmware updater was not analyzed. All analysis artifacts are in the requested scratch directory; this document is the sole project file created.

### Analyzed inputs

Original directory: C:\Program Files (x86)\FANTECH MK857 Gaming Keyboard

Scratch directory: <scratch dir>

| Input | SHA-256 |
|---|---|
| FANTECH.exe, 12,402,688 bytes | F7E3E8162E88738AF674D1896FC59C28A3FC152D94481DD1F5216BBEEA76D5D2 |
| hidapi.dll | E53F82D48C90D284D2AF9BE6F70C973461F4E1AFBA4C4D80B81576D2D9B79A66 |
| translation/English.qm | 795E865D741C149B71BB81D9F46763797208B92F6E525214AF88296C71042E1D |

**CONFIRMED:** EXE is PE32 x86, preferred image base 0x00400000; DLL disassembly uses base 0x10000000. EXE .text RVA 0x1000/file offset 0x400; .rdata RVA 0x412000/file offset 0x410E00. Addresses are preferred VAs, not runtime ASLR addresses. For example, EXE VA 0x00619650 is file offset 0x00218A50. Evidence: PE headers in scratch imports.txt.

**CONFIRMED:** embedded XML identifies XS61BRGB as MAXFIT61 with 61 keys at VA 0x009D253D; XS61RGB and XS61_SNRGB are also labeled MAXFIT61 at 0x009D2698 and 0x009D2744. VID=0416/PID=C345 is listed as type=0, nucName=M252 at 0x009FE486. These separate entries do not uniquely identify which MAXFIT61 variant is attached.

### Available analysis tools

Microsoft dumpbin 14.44.35229.0 was found at C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x86\dumpbin.exe. Node.js and rg were available. dumpbin produced imports, headers and x86 disassembly; Node read PE bytes and extracted address-bearing strings. Python/python3/py, radare2/rizin, Ghidra, llvm-objdump and strings were not on PATH; a bounded search of standard program locations found dumpbin, but no additional named disassembler. This is not proof those programs are absent everywhere. No Python package availability could be tested without a located interpreter.

Scratch evidence files: FANTECH.exe, English.qm, hidapi.dll, disasm.txt, imports.txt, hidapi-disasm.txt, hidapi-imports.txt, evidence.txt. Executables were treated solely as byte data.

## 1. Device selection and transport

| Item | Status and evidence |
|---|---|
| Enumeration | **CONFIRMED:** hid_enumerate(0,0), call 0x00619C14 through thunk 0x007E67AA; enumerate all devices, then filter. |
| VID/PID | **CONFIRMED:** compare hid_device_info words at +4/+6 against model-list values, 0x00619DE0–0x00619E06. C345 model entry cited above. |
| Accepted paths | **CONFIRMED:** path substring tests for mi_02 OR col06, 0x00619E0C–0x00619E3E; literals 0x00815A88 and 0x00815A90. |
| Open | **CONFIRMED:** hid_open_path call 0x006193F8, thunk 0x007E67B6, IAT 0x00813908. |
| Usage page/usage | **GUESSED:** 0xFF00 is plausible from the supplied device description, but unconfirmed. The inspected selector uses path strings, not a usage-page comparison. No report descriptor was obtained. |
| Output API | **CONFIRMED:** hid_write, IAT 0x00813904, thunk 0x007E67BC. Calls 0x0061B357 and 0x0061B3E9 use length 0x40. |
| Input API | **CONFIRMED:** hid_read, IAT 0x00813900, thunk 0x007E67C2. Calls 0x0061B653 and 0x0061BB4B request 0x40 bytes. |
| Feature reports | **CONFIRMED:** EXE imports neither hid_send_feature_report nor hid_get_feature_report. DLL exports both, but traced EXE traffic uses write/read. This does not prove no conceivable indirect call exists. |
| Report ID | **CONFIRMED:** constructed output byte 0 = 01. Send wrapper 0x00619650 changes it to 06 when stored path contains col06: 0x0061968B–0x006196D8, string 0x008161B8. Discovery request has equivalent adjustment at 0x0061BA89–0x0061BAD9. |
| Report size | **CONFIRMED:** application buffers and HID call length = 64 bytes INCLUDING byte 0. Device-advertised output size remains unknown; see DLL padding below. |
| Nonblocking | **CONFIRMED:** hid_set_nonblocking(handle,1) at 0x006195EF–0x006195F8 and 0x0061BAF9–0x0061BB02. |

**CONFIRMED:** the copied DLL's hid_write is VA 0x10001990. It calls WriteFile through IAT 0x10003024 at 0x100019F0. At 0x100019AD–0x100019DF it compares the caller length to the stored OutputReportByteLength and, if shorter, allocates and zero-pads to that length. Thus **64 bytes at the app API is confirmed; the precise descriptor/USB transfer length is not**. Do not assume a 65-byte buffer or remove byte 0 without descriptor evidence.

**GUESSED:** MI_02 is the intended config channel on the supplied wired keyboard. The app's explicit mi_02 acceptance strongly supports this, but no live open was performed. The col06 alternative is not evidence that this target uses report ID 06 or that col06 belongs to a particular MI_01 collection.

## 2. Common framing, checksum and response handling

All offsets below are zero-based within the 64-byte application buffer; numbers in byte layouts are hexadecimal.

| Offset | Status / meaning |
|---|---|
| 0 | **CONFIRMED:** report ID 01, or path-dependent 06. |
| 1 | **CONFIRMED:** command/opcode; callers and response dispatch inspect this byte. |
| 2 | **CONFIRMED:** command-specific selector; often zero, layer for key-table transfers. |
| 3 | **CONFIRMED:** zero in the documented builders. Not proven universally reserved. |
| 4 | **CONFIRMED:** page/chunk index for 09/18/19 builders. |
| 5 | **CONFIRMED:** payload count in 07/09/18/19 builders. Not a universal length field: 0D/0A/14 examples leave it zero. |
| 6 onward | **CONFIRMED:** command-specific data, often zero-filled remainder. |

**CONFIRMED (bounded): no application checksum is appended or calculated in the traced 0D, 15, 0A, 07, 09, 14, 18 and 19 packet paths.** For example, 07 initializes all 64 bytes, fills fields and directly queues at 0x007011A3. The generic sender copies all 64 bytes at 0x00619709–0x00619740; worker directly calls hid_write at 0x0061B357. The DLL copies/pads bytes and calls WriteFile. No sum, XOR, CRC or checksum slot is present in those paths. A5/5A described below are response sentinels, not a demonstrated checksum. A universal claim about every untraced opcode is **GUESSED**.

**CONFIRMED:** 0x00619650 is an asynchronous enqueue wrapper: it can return -1 when disconnected and returns 0 after queueing (0x00619678–0x00619686, 0x00619754). A nonnegative return/logged “Successful” is not evidence of a device acknowledgement. Actual I/O is in the worker.

**CONFIRMED:** input handling accepts positive hid_read results at 0x0061B65E–0x0061B662. Opcode 15 is recognized at 0x0061B668–0x0061B67A and forwarded as a 64-byte copy. Ordinary traffic is forwarded at 0x0061B7AE–0x0061B7B1 and dispatched by opcode. The inspected paths do not provide a robust universal response-length validator; some loops copy 64 bytes from input offset +6 despite a 64-byte allocation (e.g. 0x0061BBB6–0x0061BBDB). These are properties of the app, not recommended parser behavior.

**CONFIRMED:** general response dispatch recognizes 19 and 18 and emits corresponding signals at 0x0064D53A–0x0064D56F. **GUESSED:** exact ACK status/sequence semantics; those branches do not themselves decode a status byte. Macro send/retry code waits on state and retries a saved 64-byte packet at 0x006FB580–0x006FB79D; it compares elapsed time to 100 and retry count to 4 at 0x006FB5EA and 0x006FB645. Do not treat these constants as verified USB timing requirements.

## 3. Read device/firmware identification: 0D

**CONFIRMED:** discovery builder 0x0061BA37–0x0061BA84 zeroes 64 bytes and sets [0]=01, [1]=0D, [2]=00. It calls hid_write directly at 0x0061BAEB, enables nonblocking input and sleeps 250 ms at 0x0061BB0A–0x0061BB0F, then calls hid_read at 0x0061BB4B. A first response is accepted if its byte 1 is 0D (0x0061BB66–0x0061BB7B).

**CONFIRMED:** response text begins at offset 6, converted using %s (literal 0x00815D50) at 0x0061BBBF–0x0061BC0B. Parsing tests ':' and splits on ',' (literals 0x00815D58/5C/60; code 0x0061BCD3–0x0061BE56). Another read at 0x0061C2E4 consumes text at offset 6, split on ','; first four characters and characters 4–5 are taken at 0x0061C41A–0x0061C444, and date/month parsing compares Jan at 0x0061C4CD. This second response path does not visibly recheck opcode 0D before parsing.

**GUESSED:** these text replies carry model/firmware/build identification; exact grammar and a numeric firmware-version field are unresolved. No real device reply or version number was observed. The updater filename V10307 does not prove the attached keyboard's firmware version.

Full 64-byte request (**CONFIRMED builder output**, default report ID):

```text
01 0D 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

## 4. Serial number: 15

**CONFIRMED:** after opening, 0x006194D5–0x00619508 constructs zero-filled [01,15,...] and queues it. String 0x00815E98 names this “Inquire [Device SN 0x15]”. Receive recognition is 0x0061B668–0x0061B73C; string 0x00815FA0 says “hid_read Read 0x15”. **GUESSED:** SN payload byte encoding, length and its relationship to USB iSerialNumber are unresolved.

Full request:

```text
01 15 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

## 5. Lighting capability query: 0A

**CONFIRMED:** builder at 0x0064A430–0x0064A4C9 emits zero-filled [01,0A,...]; failure literal 0x0081CE60 names “Inquire [Light Mode]”.

**CONFIRMED:** parser at 0x0064CA30–0x0064CC18 recognizes opcode 0A (and opcode 02 for a separate lighting list), starts at offset 6, collects byte values until A5 or 5A, then takes the following two bytes as speed maximum and brightness maximum. The names are evidenced by strings “SpeedMax” at 0x0081D61C and “BrightnessMax” at 0x0081D628 and handler 0x0064DCB0. Sentinel itself is omitted from the list. Equivalent parser 0x0064DF20 reads the same structure for additional lighting lists.

**GUESSED:** collected values are supported effect IDs, strongly supported by the Light Mode log at 0x0081D220. Mapping each ID to the Maxfit61's effect names requires additional tracing or an actual response. A5 versus 5A distinction, capability response bounds and what follows the two maxima are unresolved.

Full request:

```text
01 0A 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

**GUESSED / unresolved:** a GET CURRENT EFFECT command. 0A is a capability/list query; it is not established as readback of current mode, color, speed or brightness. The app also persists lighting fields in local profiles; those are not proof of device readback.

## 6. Set keyboard lighting effect: 07

**CONFIRMED:** builder 0x00700E60–0x007011A3. It accepts a 14-dword lighting struct by value; named fields are established by serializer 0x007C395D–0x007C3A46 and deserializer 0x007C3E1E onward. Field-name strings are at 0x008594BC–0x00859560. The 56-byte struct copy can also be seen at 0x00723720–0x00723741. UI/log evidence includes “Brightness” (0x00817E60), “Speed” (0x00817EBC), “Direction” (0x0081B17C), and “Write Led” (0x0083B8BC).

| Byte | CONFIRMED value / named source | Evidence VA |
|---|---|---|
| 0 | 01, with generic col06 override | 0x00700EA9 |
| 1 | 07 | 0x00700EB6 |
| 2–4 | 00 | 0x00700ED5–0x00700F15 |
| 5 | 0E (14 bytes of effect data, offsets 6–19) | 0x00700F22 |
| 6 | ledType | 0x00700F2F–0x00700F32 |
| 7 | ledBrightness | 0x00700F3E–0x00700F41 |
| 8 | ledSpeed | 0x00700F4D–0x00700F50 |
| 9–11 | ledModeR, ledModeG, ledModeB | 0x00700F91–0x00700FBA; 0x007010A9–0x007010D2 |
| 12–14 | ledBgR, ledBgG, ledBgB | 0x00700FBE–0x00700FE7; 0x007010D6–0x007010FF |
| 15 | ledDirection | 0x00701103–0x0070110E |
| 16 | ledModeType | 0x00701112–0x0070111D |
| 17 | ledOpenOrClose | 0x00701121–0x0070112C |
| 18–63 | 00; includes two zero bytes within declared 0E payload | 0x00701130–0x0070118F |

**CONFIRMED:** ledType 00 forces ledModeType to 00 at 0x00700F54–0x00700F60. For types 05,06,07,08,09,0B,0C the six RGB bytes are copied directly regardless of ledModeType (0x00700F67–0x00700FEB). For other types, ledModeType==1 forces bytes 9–14 to FF 00 00 00 00 00; otherwise RGB is copied (0x00700FF0–0x00701103). Therefore it would be incorrect to assume modeType is always just an arbitrary color-enable flag.

**GUESSED / unresolved:** static-effect numeric ID, usable brightness/speed ranges for the target, direction enum values, meaning of modeType values, and on/off polarity. Capability query provides runtime maxima; no target response was read. Names “ledOpenOrClose” alone do not prove 0=off or 1=on. ledKeyboardOrSide is the struct's first field but this particular 07 builder does not copy it into the report.

### Static red at full brightness

A verified full numeric report for that named operation cannot be supplied without inventing the static-mode ID, target brightness maximum and on/off polarity. The **GUESSED semantic template** is:

```text
01 07 00 00 00 0E STATIC_ID BRIGHTNESS_MAX SPEED FF 00 00 00 00 00 DIRECTION MODE_TYPE OPEN_CLOSE 00 00 ... 00
```

This template is not a sendable byte report; uppercase identifiers are unresolved parameters. The positions and foreground RGB bytes are confirmed, but visible behavior and chosen settings are not. Concrete, fully numeric red data is available for the per-key chunk builder below.

## 7. Per-key RGB/custom lighting: 09

**CONFIRMED:** builder 0x006F13A0–0x006F14C0 creates:

```text
[0] report ID
[1] 09
[2] low byte of argument 1 (selector, semantics unresolved)
[3] 00
[4] low byte of argument 3 (page/chunk index)
[5] number of data bytes
[6..] low byte of each supplied list element
remaining bytes zero
```

Evidence: selector 0x006F1403–0x006F1406; page 0x006F1432–0x006F1435; count 0x006F1439–0x006F1449; payload 0x006F1497–0x006F14A2; log “Write custom-Led” at 0x0083ADCC.

**CONFIRMED:** caller 0x006F10F0 walks a 6-row by 22-column software matrix, obtains QColor red/green/blue and appends them in that order (0x006F11A0–0x006F11F1; imported QColor accessors in imports.txt). It sends chunks of 18 cells = 54 RGB bytes (0x006F1204–0x006F1240), indexed from zero, and a final chunk of 6 cells = 18 bytes (0x006F1258–0x006F1295). The full matrix therefore requires seven 54-byte chunks and one 18-byte chunk. The caller passes zero for the builder's second argument, which is not copied to the report.

**GUESSED / unresolved:** physical key-to-matrix mapping for the 61-key variant, selector meaning, persistent versus preview behavior and any required activation/commit commands. The 132 software cells include layout gaps; they are not evidence of 132 physical LEDs. The app has lighting synchronization code/logs (e.g. onSendLightSynchronize0x17Cmd at 0x0082175C), but its complete activation sequence is not decoded here.

Concrete full 64-byte **CONFIRMED builder-derived example**: command 09, selector argument 0, page 0, first 18 software cells supplied as RGB red (FF 00 00). This is a parameterized builder output, not captured traffic and not a claim that a lone packet yields persistent static red:

```text
01 09 00 00 00 36 FF 00 00 FF 00 00 FF 00 00 FF
00 00 FF 00 00 FF 00 00 FF 00 00 FF 00 00 FF 00
00 FF 00 00 FF 00 00 FF 00 00 FF 00 00 FF 00 00
FF 00 00 FF 00 00 FF 00 00 FF 00 00 00 00 00 00
```

## 8. Onboard key remapping and Fn layer: 18

**CONFIRMED:** key-table chunk builder 0x006F9F10–0x006FA065 creates:

| Offset | Value |
|---|---|
| 0 | 01 / col06 override 06 |
| 1 | 18 |
| 2 | layer/selector argument |
| 3 | 00 |
| 4 | page argument |
| 5 | number of payload bytes |
| 6 onward | supplied table bytes |
| remainder | 00 |

Evidence: opcode at 0x006F9F8C; selector at 0x006F9F98–0x006F9F9B; page at 0x006F9FC7–0x006F9FCA; count at 0x006F9FD1–0x006F9FDE; copy at 0x006FA024–0x006FA037. “Write keyboard Onboard-data” string is 0x0083BE34.

**CONFIRMED:** caller 0x00714E80 onward builds action records and calls this writer at 0x00715827 and 0x0071589D. It groups 14 key slots per packet (0x007157CC–0x007157E7). The builder actions generally produce four bytes per key, giving 56 payload bytes per full page. Default pass uses selector 0, then a second pass uses selector 1 (0x0071505C–0x0071507B, 0x00715985); logs identify “Download Over [Default]” at 0x0083B480 and “[FN1]” at 0x0083B510.

**CONFIRMED:** the app distinguishes action categories using a five-entry switch at 0x0071527F–0x00715292 (table 0x00715DD8). Branches construct literal tags including 10 (0x0071542B), 13 (0x007154E1), 07 (0x00715583), and 12 (0x00715618); normal/special branches and blank slots also exist. **GUESSED / unresolved:** full record field order and category meanings, modifier encoding, physical slot ordering, macro-reference encoding and safe handling of the physical Fn key. These tags are not presented as a complete remapping grammar. No concrete A-to-B remap report is justified yet.

**CONFIRMED:** separate generic app strings name “Remap key”, “Macro” and “Switch profile”; English.qm source-text offsets are 0xC9F, 0xACE and 0xDFC. App text at 0x0082C620 says onboard key reassignments are saved locally onto MAXFIT61. This supports the feature names, not a particular wire encoding.

## 9. Macros: 19

**CONFIRMED:** the macro-upload builder constructs opcode 19 at 0x006FAED8, uses a caller-supplied selector at byte 2 (0x006FAEE4–0x006FAEEA), zero at byte 3, chunk index at byte 4 (0x006FAF1F–0x006FAF25), and count 3A at byte 5 (0x006FAF31). First packet contains eight metadata bytes at offsets 6–13 and up to 50 event-data bytes at offset 14; subsequent chunks copy up to 58 event bytes at offset 6. See 0x006FAF36–0x006FB332. Metadata includes transformed input values; exact semantic meaning is unresolved.

**CONFIRMED:** builder can pad intermediate pages with zero-filled [01,19,selector,00,page,3A,...] (0x006FB3B9–0x006FB460) and emits a final page with [4]=04 and [5]=18, zero-filled payload (0x006FB479–0x006FB51B). Do not infer all macro commands have these count values.

**CONFIRMED:** send/retry helper 0x006FB580 preserves and resends the 64-byte packet; “Write keyboard Macro-data list” string is 0x0083D944. Input opcode 19 emits a signal at 0x0064D53A–0x0064D551.

**GUESSED / unresolved:** event opcodes, key down/up encoding, delay units/width, repeat modes, macro slot numbering, meaning of first-packet metadata, final-page semantics and interaction with 18 key bindings. These must be resolved before claiming a usable macro implementation. Translation evidence confirms “Macro record” (English.qm 0x1239), “Delay setting” (0x11F7), “Play Number” (0x113A), and “Continous playback on/off” (0x10D7), but supplies no bytes.

## 10. Polling rate

**CONFIRMED (shared app, explicitly named mouse):** handler 0x00708810 constructs [0]=01, [1]=46, [2..4]=00, [5]=01, [6]=rate code and remaining zeros. Four input indices select 08,04,02,01 at 0x00708922–0x0070896D; jump table 0x00708A7C. Logs explicitly say “Mouse RateReturn” at 0x0083BD1C/0x0083BD90; Qt metadata says onCurIndexChanged_MouseRateNumIndex at 0x00F05681.

**GUESSED:** interpreting those bytes as 8/4/2/1 ms (125/250/500/1000 Hz) is plausible but not independently established here. **GUESSED / unresolved:** a keyboard polling-rate command or evidence that Maxfit61 accepts opcode 46. Do not use this mouse handler as a confirmed Maxfit61 command.

## 11. Profiles and factory reset

### Factory reset

**CONFIRMED:** factory-reset flow builds opcode 14 with byte 6=01 and all other payload bytes zero, 0x007166B0–0x007167DA; adjacent logs say “[Reset]” at 0x0083A9E0/0x0083AA10 and “Restore the factory settings successful” at 0x0083AA44.

**CONFIRMED:** the current-profile reset flow also emits the same packet at 0x00708553–0x0070868F after other app operations. Therefore opcode 14 is a reset primitive; the exact breadth of firmware-side state erased is **GUESSED**, and cannot be distinguished from those log messages alone.

Full 64-byte factory-reset-flow report (**CONFIRMED builder output**, never sent):

```text
01 14 00 00 00 00 01 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

### Profile switching

**CONFIRMED:** profile support exists in UI/metadata: “Switch profile” at English.qm 0xDFC, “Assign proFile” at 0x40D, “Profile 1” at EXE 0x0083B0BC, “onProfileItemMouseClicked” at 0x00F05445 and “onOnboardProfileComboxCurrentIndexChanged” at 0x00F057BC.

**GUESSED / unresolved:** a standalone device profile-switch opcode, onboard profile count/index encoding, and whether switching a particular UI profile reuploads configuration rather than selecting a hardware bank. Byte 2 of 18 is established as a Default/FN1 selector in the traced caller; it must not simply be renamed “profile ID”.

## 12. Battery query

**CONFIRMED (negative search result):** no ASCII string matching battery, batt, charge or wireless was found in the extracted EXE ASCII strings; no battery UI/query was identified in the inspected code. The translation source strings inspected did not reveal a battery feature. No battery opcode was recovered.

**GUESSED:** no battery query is needed or supported by this wired target, consistent with the user-supplied wired description. Static negative search cannot prove a command is absent from all firmware. Battery state must be reported as unsupported/unknown, not fabricated as 100% or mapped to an unrelated opcode.

## 13. Remaining work and confidence

| Finding | Confidence |
|---|---|
| EXE write/read transport, 64-byte app buffers, report-ID construction and col06 override | High, code-confirmed |
| Basic 0D/15/0A request bytes; 07 field positions; 09 RGB chunk construction; 14 reset-flow packet | High for app behavior; untested on hardware |
| 18/19 chunk framing and Default/FN1 passes | High for traced app code; incomplete payload semantics |
| FF00 usage page, exact device descriptor length, effect ID/name mapping, static red full brightness, profile switch, keyboard polling rate, complete macros | Unresolved / GUESSED |

Overall confidence: **medium for a partial Maxfit61 protocol spec; insufficient for a complete device driver**. Limits are unresolved semantics/model applicability, not lack of a disassembler. Completion would require further static tracing of effect-name mapping, key/macro serialization and profile handlers, or separately authorized descriptor/traffic evidence. No such live/device operations were performed in this task.
