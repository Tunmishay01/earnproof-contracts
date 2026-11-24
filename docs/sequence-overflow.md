# Monotonic Sequence Overflow Policy

EarnProof treats every stored sequence and history count as a checked,
monotonic value. A transition reserves its next value before writing contract
state. If the current value is the integer maximum, the transition fails and
leaves storage, TTLs, and events unchanged.

## Covered counters

| Contract | Counter | Width | Overflow result |
| --- | --- | --- | --- |
| Issuer registry | signing-key usage sequence | `u64` | `SequenceOverflow` |
| Issuer registry | compromised-key history count | `u32` | `SequenceOverflow` |
| Issuer registry | address-rotation history count | `u32` | `SequenceOverflow` |
| Issuer registry | issuer epoch | `u32` | `SequenceOverflow` |
| Protocol config | pause acknowledgement count | `u32` | `InvalidState` |
| Proof registry | pause acknowledgement count | `u32` | `InvalidState` |
| Proof registry | upgrade history count | `u32` | explicit overflow panic |
| Proof registry | registry epoch | `u32` | explicit overflow panic |

At `MAX - 1`, one transition succeeds and stores `MAX`. At `MAX`, the next
transition fails before its first write. Failed validation, authorization, and
replayed transitions likewise do not advance any counter.

The error variants above preserve each contract's existing public interface.
No counter wraps, saturates, or silently reuses an earlier sequence.
