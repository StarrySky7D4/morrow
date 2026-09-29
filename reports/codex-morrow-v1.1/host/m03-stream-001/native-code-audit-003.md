# Host003 response-code audit

Scope: read-only review of every host emission site against the frozen v3 codec and
plugin002 admission/session. Only Welcome changed from legacy code2 to success code0.
No wire/schema/business contract changes. Frozen host002/plugin002 remain unchanged.

| Host frame | Envelope code | Sequence / meaning |
|---|---|---|
| Challenge | 0 | initial control 0 |
| Welcome | 0 after this fix | exact Hello response 1 |
| DataOffer | 0 | unsolicited control 0 |
| DataBound | 0 | data lane 1 |
| HttpProposed, HttpApproved | 0 | unsolicited control 0 |
| State | 0 via state() | echoes Prepare, Commit, Query or Close request |
| CreditState, CancelAccepted | 0 via state() | echoes Credit/Cancel request |
| ResponseHead | 0 | unsolicited control 0; HTTP status is Head.status |
| BodyChunk | 0 | separate data sequence starts at 2 |
| HttpTerminal, RequestClosed | 0 via state() | unsolicited control 0; failure is Progress.error_code |
| Stop | nonzero reason | unsolicited control 0; failure/requested-stop reason |
| Denied | nonzero reason | echoes rejected request sequence |

Plugin002 admission requires Challenge/Welcome success code0, data code0, and nonzero code
only on the explicitly allowed status/error kinds. The host uses 0 for these status envelopes
and keeps domain failure in Progress.error_code. CancelAccepted itself closes admission;
its envelope code0 does not mean HTTP business success. Stop25 is a requested stop, not success.
Network errors map Limit21/Timeout20/Cancelled19/other26. Generic request-control failures
currently map Denied27; this audit does not claim fine-grained failure taxonomy coverage.

Direct-response sequence and notification-zero emission order were checked. Commit State is
queued before ResponseHead; data has its independent counter. Frame validity alone does not
prove runtime ordering or real HTTP acceptance.

Strict synthetic peer now rejects Welcome unless kind Welcome, sequence1 and code0. Exactly
one new real Hello/Welcome/Close positive case was run; old two negative cases, API and component
suites were not rerun. No HTTP worker or real plugin was started by this check.

Evidence: native-build-20260928T225754496446Z/result.json and
native-welcome-20260928T225819377451Z/result.json. Both processes exit0, complete Close ACK
received before stdin EOF, owner release proven. Full HTTP/Core pairing still pending.
