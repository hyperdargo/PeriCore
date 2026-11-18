# AJAZZ receiver HID protocol (static analysis)

Binary: `iot_driver.exe`, 6,698,760 bytes. SHA-256: `2B8BCAAB6186CDE1918863830496ADAB9FF1B6E49FFC5157AC6227D8CA416FB8`. PE32 x86, preferred image base `0x00400000`. Addresses below are preferred-image VAs; subtract `0x00400000` for RVAs. ASLR may relocate them. Only the scratch copy and dumpbin output were read; the vendor helper was never executed and no device operations were performed.

**CONFIRMED** means directly supported by binary instructions/data. **GUESSED** marks interpretation or incomplete tracing. The hidapi entry points are exports implemented inside this EXE, not imported hidapi functions.

## Receiver target selection

**CONFIRMED:** receiver routing uses a separate, cached `F6` transaction, not an extra report ID or a target byte inserted into every original message.

| Target | Internal selector | Logical selection message |
| --- | --- | --- |
| KEYBOARD | 0 | `F6 0A` |
| MOUSE | 1 | `F6 05` |
| Unresolved/default | 2 | `F6 0D` (GUESSED meaning) |

At `0x005899B7–0x005899EA`, request enum 2 (MOUSE) becomes boolean selector 1 via `sete bl`; keyboard becomes 0. The receiver send constructor/body are `0x00572DD0` / `0x00572EE0`, invoking `0x0056BB30`. That state machine calls the selector at `0x0056BFEB`, then sends the original message at `0x0056C0C0`.

`0x0056AE4E–0x0056AE58` compares the requested selector with cached byte `[device+0x9D]`. On change, `0x0056AE81–0x0056AECE` emits `F6` plus `(0x000D050A >> (selector*8)) & 0xFF`, with checksum NONE (word `0x0200` at `0x0056AEB7`). After success it waits 10 ms (`0x0056AF27–0x0056AF4C`) and updates the cache (`0x0056AFCD–0x0056AFD3`). Repeated requests for the same target can omit the selection write.

**GUESSED/incomplete:** the full protobuf NONE=0 dispatch branch was not traced end-to-end. Ordinary raw send `0x005761F0 -> 0x005762F0 -> 0x00569E40` has message, length and checksum arguments and does not perform this F6 transaction. Protobuf NONE=0, internal selector 2, and device classification byte offset 0x26 are different concepts.

## Framing: significant 67-versus-65 discrepancy

**CONFIRMED:** shared sender `0x00569E40` copies the message, then resizes it to **66 bytes** with zero fill (`push 0; push 42h; call 00876FC0` at `0x00569FF4–0x00569FFC`). This is resize, not reserve: `0x0087701B` stores the requested length into the vector. Longer input is truncated to 66 bytes.

Normal feature sender `0x00567FC0` appends a zero first (`0x00568009–0x0056801E`), then the payload (`0x00568026–0x00568030`), then sends through `0x00703000 -> 0x008B0720`. Therefore the traced hidapi buffer is:

```text
H[0] = 00                 report ID
H[1..67] = P[0..66]        66-byte padded payload
hid_send_feature_report length = 67 (0x43)
```

The normal vtable is `0x008E5258`: send slot `0x008E5264 = 0x00567FC0`, read slot `0x008E5268 = 0x005682A0`. The shared sender invokes the send slot at `0x0056A135`.

**CONFIRMED:** hidapi does not truncate this to the descriptor's feature length. At `0x008B072B–0x008B0739`, caller lengths at least as large as the feature length pass unchanged; only shorter buffers are padded (`0x008B073B–0x008B0772`). The API call occurs at `0x008B0779`.

**GUESSED:** physical USB transfer behavior for this overlength buffer against the user-supplied 65-byte descriptor. Static analysis proves the API buffer construction, not what Windows/device ultimately transfers or rejects. The helper's exact send buffer cannot honestly be presented as 65 bytes.

## BIT7 / BIT8 checksum

**CONFIRMED:** these modes count bytes; they do not select a 7-bit mask. Arithmetic wraps in `dh`, followed by `not dh`.

For device classification byte `[device+0x26] == 2`:

```text
BIT7 (0): P[7] = 0xFF - (sum(P[0..7]) & 0xFF)
BIT8 (1): P[8] = 0xFF - (sum(P[0..8]) & 0xFF)
NONE (2): no checksum write
```

Ranges are half-open. BIT7 sums indices 0–6; BIT8 sums 0–7. Report ID is excluded. In H, the checksum locations are 8 and 9 respectively.

Evidence: mode tests `0x0056A007–0x0056A015`; classification checks `0x0056A020–0x0056A027`, `0x0056A068–0x0056A06F`; eight adds `0x0056A098–0x0056A0AC`; seven adds `0x0056A0DF–0x0056A0F0`; complement/stores `0x0056A0B2`, `0x0056A0F6`, `0x0056A103`, `0x0056A11F`.

**CONFIRMED:** classification !=2 shifts the checksum window by one: BIT7 sums P[1..8], stores P[8]; BIT8 sums P[1..9], stores P[9]. See `lea esi,[eax+1]` at `0x0056A095`, `0x0056A0DC`, and stores `0x0056A111`, `0x0056A11F`. This routine does not itself insert that leading payload byte. **GUESSED/incomplete:** classification mapping for every wired/BLE product, including PID 4026.

Computed no-prefix examples (66-byte payloads; prepend report ID 00):

```text
F1, BIT7: F1 00 00 00 00 00 00 0E + 58 zeros
82, BIT7: 82 00 00 00 00 00 00 7D + 58 zeros
F1, BIT8: F1 00 00 00 00 00 00 00 0E + 57 zeros
```

The F1/82 command meanings come from the task; these examples confirm checksum arithmetic, not device execution.

## readRawFeature

**CONFIRMED normal path:** `0x00576470 -> 0x00576550 -> 0x0056A310 -> 0x005682A0 -> 0x007030C0 -> 0x008AFC90 (hid_get_feature_report)`.

`0x005682D0–0x005682FB` allocates **65 zero bytes**, so report ID is **0**. The call is at `0x00568355`. The successful normal reader removes exactly the first byte via a split at 1 (`0x0056838D–0x00568394`) and returns the remaining **64 bytes**. It does not trim the vector to the API return count; zero padding remains. Slice reconstruction is at `0x00568487–0x005684D7`.

The Windows implementation selects IOCTL `0x000B0192` (`0x008AFC98`) and uses DeviceIoControl/completion wait at `0x008AFDA0`. It adds one to the returned count for report ID zero (`0x008AFE3A–0x008AFE43`). This does not change the fixed allocation or caller's stripping step.

**CONFIRMED:** there is no unconditional delay in normal implementation `0x005682A0`; shared layer `0x0056A310` can wait in 10 ms steps on its device busy flag. This feature path does not use hid_read. A distinct input path `0x00568970` allocates 66 bytes and uses a 1000 ms read timeout (`0x00568A21–0x00568A2A`).

**CONFIRMED receiver-aware read:** `0x005730F0 -> 0x00573130 -> 0x0056C520` checks receiver status; it can send logical `FC` with NONE checksum (`0x0056CA1F–0x0056CA5F`, command constant `0x008E5504`), wait 10 ms, then invoke the shared feature reader (`0x0056CBE9`). Status retries use 100 ms, up to 15 attempts (`0x0056C606`, `0x0056C750–0x0056C789`). **GUESSED:** firmware meaning of FC is reply retrieval/acknowledgment. **GUESSED/incomplete:** all gRPC read dispatch variants are not fully mapped.

## Receiver battery/online polling

**CONFIRMED:** the common status transaction is logical `F7` with checksum NONE. `0x0057672A–0x0057673B` installs the one-byte constant at `0x008E6154` and NONE mode; `0x00576755` invokes transaction `0x0056A5E0`. That sends through `0x00569E40`, waits **1 ms** (`0x0056A83C–0x0056A864`), then reads a feature report (`0x0056A8C3`). One transaction obtains both batteries; it is not the ordinary keyboard `[82,battery,state,lowpower]` layout.

Let r be the result after normal report-ID removal:

| Offset | CONFIRMED interpretation/use | Evidence VA |
| --- | --- | --- |
| r[0] | is_ready_read: equals 1 | 0057696D, 00576977–0057697A |
| r[1] | keyboard battery, zero-extended u32 | 00576997, 00576AC7–00576AD0 |
| r[2] | mouse battery, zero-extended u32 | 00576993, 00576ACD, 00576AE5 |
| r[3] | keyboard online test: equals 0, when status present | 00576983–0057698A |
| r[4] | mouse online test: equals 0 in type-2 branch | 00576A64–00576A7B |
| r[5] | write-finish test: equals 1, assigned according to r[6] | 00576970, 0057697D–00576980, 00576A50–00576A82 |
| r[6] | discriminant controlling status availability/write completion | 00576973, 0057698D–005769BA |

**CONFIRMED naming evidence:** formatter `0x0055D680` identifies Status24 fields: structure offset 0 battery, 4 is_ready_read, 5 is_online, 6 is_write_finish, using strings at `0x008E608C–0x008E60CF`. Formatter `0x0055D0D0` identifies common keyboard_status at offset 0x10 and mouse_status at 0x18. `0x005737CD–0x005737E9` places the two decoded records into the common result, and `0x0057397D–0x00573983` copies them into cached common status.

**CONFIRMED caveats:** r[6] branches on 1/2/3/other and synthesizes unknown/unavailable statuses (including internal option discriminant 2). Do not interpret online bytes unconditionally for an absent status. An all-zero reply takes a retry path (`0x00576947–0x00576961`, `0x005769EF–0x00576A46`). Values are copied directly, with no 0–100 conversion/clamping in this decoder; interpreting battery as percent follows the application context.

**GUESSED/unresolved:** complete firmware meaning of each r[6] value. **GUESSED/unresolved:** a distinct charging flag in this receiver reply. r[5] is grounded as **write completion**, not charging; no charging field was established.

**CONFIRMED interval:** watch loop `0x0044FAE1–0x004506E7` polls common status through future `0x005732C0` / vtable `0x008E5EF0`, compares old/new states and logs `dangle status ====>>>>` at `0x0045038F`. It then constructs a **2-second sleep** at `0x0045065A–0x00450669` (seconds 2, nanoseconds 0). There is also a 2-second backoff at `0x0044F92F–0x0044F93B`. Transaction/retry time adds to the start-to-start interval.

## Concrete mouse-battery request

**CONFIRMED:** query the receiver with F7 and read mouse battery from logical r[2] (feature buffer offset 3). This query returns both batteries and does not require a preceding mouse-target selection.

Full traced **67-byte** normal hidapi send buffer:

```text
00 F7 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00
```

Get-feature separately uses 65 zero bytes initially (report ID 0), returning the 64-byte stripped result.

**GUESSED descriptor-compatible 65-byte variant, not exact helper behavior:** use the same four 16-byte rows above followed by one 00 rather than three. Whether this shorter send works is not established statically.

**CONFIRMED routing example for other mouse commands:** on target change send a 67-byte buffer beginning `00 F6 05`, followed by 64 zeros; wait 10 ms; send the original message padded/checksummed separately. F6 has no checksum.

**GUESSED illustrative reply:** `01 64 49 00 00 01 02 ...` contains keyboard battery 100 and mouse battery 73, ready=true, and mouse online/write-finish=true in the type-2 branch. This is invented to demonstrate offsets, not a capture; keyboard validity depends on the discriminant.

## Additional controls and open handshake

**CONFIRMED traffic generators:**

- F6 selection as above.
- `FE nn`: two-byte control built at `0x0056ABE0–0x0056AC17`, NONE checksum. Receiver operations call it with 8 at `0x00574854–0x0057486D` and 12 at `0x00574E10–0x00574E2E`. **GUESSED:** firmware mode/transfer-setup semantics.
- FC receiver read control as above.
- `8F`: internal identity transaction at `0x0056DA15–0x0056DA58`, constant `0x008E5638`. Identifier processing sets bit 31 at `0x0056DAFE–0x0056DB12`. Distinct from the task's F1 query.
- `FD`: one-byte transaction at `0x0057500D–0x00575032`, constant `0x008E536C`; **GUESSED:** update-related role. This constant is FD, not F1.

**GUESSED/unresolved:** exhaustive unconditional device-open handshake, including whether every receiver open sends F1 before polling. The controls above are verified code paths, but are not all proven unconditional at open. No claim of “no handshake” is made.

## Outstanding boundaries

Further tracing is needed for the complete gRPC NONE/read dispatch variants, r[6] firmware enum, charging information and exhaustive open traffic. Physical handling of the overlength send remains unverified. No USB capture was obtained or device traffic generated. Core checksum, F6 routing, F7 request/battery offsets, report-ID handling and two-second watch delay have direct static evidence.
