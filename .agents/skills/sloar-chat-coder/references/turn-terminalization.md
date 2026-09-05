# Bounded turn terminalization

## Why this exists

A chat-based coding agent can fail to finish a response even when the host itself is healthy. The agent may keep extending its own work because one required gate is RED, one more diagnostic seems useful, or each diagnostic reveals another possible follow-up.

This is different from a host/runtime stall. The failure mode is **self-extension**:

```text
required gate RED
  -> one more diagnostic
  -> one more edit
  -> one more verification
  -> another follow-up appears
  -> visible answer never reaches a terminal report
```

Sloar must not treat "keep working until everything is green" as permission for an unbounded turn.

## Core rule

**A RED or unavailable required gate changes the terminal status; it does not remove the obligation to terminate the turn.**

A turn may end as:

```text
COMPLETED
PARTIAL
BLOCKED
FAILED
```

`PARTIAL`, `BLOCKED`, and `FAILED` are valid terminal outcomes. They are preferable to an indefinitely ACTIVE turn whose remaining uncertainty is already known.

## Default corrective cycle

Unless the target repository defines a stricter or more specific policy, one failure fingerprint gets one bounded corrective cycle:

```text
1. diagnose from concrete failure evidence
2. make at most one corrective change for that diagnosis
3. re-run the affected verification once
```

After that revalidation:

- if the gate is GREEN, continue normally;
- if the same failure fingerprint remains, terminalize `PARTIAL` or `BLOCKED` and report it;
- if the gate is blocked by an external dependency, permission, unavailable artifact, provider outage, or equivalent condition the current turn cannot safely change, terminalize `BLOCKED`;
- if a genuinely different failure fingerprint appears because new evidence changed the diagnosis, one new bounded cycle may begin for that new fingerprint.

Do not rename or slightly rephrase the same failure to manufacture a "new" fingerprint.

The target repository may explicitly allow more attempts, but Sloar still requires a named bound or differentiated evidence. No retry rule may silently renew itself.

## No recursive "one more check"

The following is not a valid continuation policy:

```text
"I'll inspect one more log"
"I'll check one more endpoint"
"I'll wait for one more CI run"
"I'll fix this one last thing"
```

when each step can recursively create another equivalent step without a new bounded plan.

Before starting a follow-up after a failed revalidation, ask internally:

```text
Did new evidence materially change the failure fingerprint or acceptance scope?
```

If no, terminalize the current turn.

## Required gate versus optional improvement

Separate acceptance-critical work from opportunistic follow-up.

If the original requested acceptance target is satisfied and a newly discovered issue is:

- unrelated to the current change;
- pre-existing;
- optional hardening;
- an improvement beyond the user's requested scope;
- or a new product problem that deserves its own diagnosis;

then do not silently extend the current turn. Record it as pending/limitation/next action and return the current result.

A direct regression introduced by the current change may stay in the current turn, but it is still subject to the bounded corrective-cycle rule.

## Long-running CI and external waits

Waiting is work only while it has a bounded purpose.

If a required remote check is queued, running, rate-limited, waiting on an external model/CDN/provider, or otherwise not producing new evidence:

1. preserve the exact run/request/artifact identity when available;
2. perform only the repository-defined bounded wait/poll strategy;
3. if the repository defines no wait strategy, do not poll indefinitely;
4. terminalize `PARTIAL` or `BLOCKED` with the current remote state and one next action.

A check still running is not a reason to keep the visible turn open for hours or days.

Sloar must never claim the pending check passed. It may report that all earlier gates are GREEN and the remaining gate is pending or blocked.

## Autonomous / ULW-style requests

A request for deep autonomous work permits more substantial implementation before reporting. It does **not** disable terminalization.

Autonomous work can traverse multiple distinct failure fingerprints when each one has new evidence and a bounded plan. It must still stop the current turn when:

- the same fingerprint survives its bounded corrective cycle;
- the remaining blocker is external or requires user choice/authorization;
- the task has reached a safe meaningful milestone and the next work is a separate scope;
- continued waiting would only poll unchanged state;
- or the repository's own stop condition is reached.

Do not interpret `ULW`, "finish it", "keep going", or similar language as permission for an infinite retry/wait loop.

## Terminalization record

When durable turn state is active, a non-COMPLETED terminal snapshot should preserve enough information to explain why the turn returned:

```text
terminal status
termination reason
failed/pending gate
failure fingerprint when relevant
last evidence observed
completed work
remaining work
one safe next action
```

The final visible response should be concise. It should tell the user what finished, what did not, and why the turn stopped. Do not dump internal Sloar mechanics unless they explain the limitation.

## Example

Suppose UI, hand tracking, and face tracking checks are GREEN, but a final MediaPipe model-generation CI gate is RED.

Correct behavior:

```text
read the failing model-generation log
  -> diagnose model API/source issue
  -> one corrective edit if source can safely fix it
  -> rerun that affected gate once

still RED with same fingerprint
  -> terminalize PARTIAL/BLOCKED
  -> report: earlier gates GREEN; deployment intentionally blocked; exact remaining gate/failure; next action
```

Incorrect behavior:

```text
RED
  -> keep searching websites
  -> inspect another log
  -> trigger another run
  -> inspect another provider
  -> wait again
  -> never send a terminal response
```

## Relationship to interrupted-turn recovery

`operational-continuity.md` protects recoverability when a response/session is interrupted.

This contract protects **termination discipline while the agent is still capable of acting**.

Both are needed:

```text
bounded terminalization
  -> prevents avoidable self-extended turns

terminal snapshot + takeover fencing
  -> recovers when the host/session still fails anyway
```
