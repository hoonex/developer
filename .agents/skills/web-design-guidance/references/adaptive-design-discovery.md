# Adaptive design discovery contract

## Goal

Turn vague web requests into an intentional, buildable design direction without forcing users to know design terminology or answer a fixed questionnaire.

The agent must decide **whether to ask, what to ask, and how many questions are worth asking**. The purpose is to reduce expensive wrong-direction work, not to delay implementation.

## Core rule

Do not equate `user did not name a design style` with `user has no usable preference`.

A user may say:

```text
부드럽고 미래적인데 너무 SF 같지는 않게
학생들이 매일 빨리 보는 용도
버튼 누를 때 손맛이 있었으면 좋겠어
```

Those statements are sufficient design evidence even though they do not name `glassmorphism`, `minimalism`, `tactile UI`, or `microinteraction`.

Translate ordinary language into design axes internally. Do not require the user to speak designer jargon.

## 1. Build an ambiguity map

For substantial new UI or a material redesign, classify each relevant dimension as:

```text
KNOWN     explicitly established by user/repository/product evidence
INFERRED  reasonably supported but not explicit
UNKNOWN   multiple materially different choices remain plausible
```

Typical dimensions:

- product purpose and business/user outcome;
- primary audience and primary user job;
- surface type and information architecture;
- target platform and dominant viewport/input mode;
- existing brand/design system to preserve;
- information density and content/data volume;
- desired emotional tone / impression;
- visual/material direction;
- interaction intensity and direct-manipulation needs;
- motion intensity;
- imagery/data-visualization needs;
- accessibility/localization constraints;
- references the user wants to resemble or explicitly avoid.

Do not demand all dimensions for every task. A settings panel and a campaign landing page have different ambiguity risks.

## 2. Decide whether a question is worth asking

Ask only when the answer can materially change the implementation and choosing wrong would be expensive to reverse.

Use this reasoning model:

```text
question value
≈ decision impact × uncertainty × rework cost
  ÷ reversibility
```

This is a decision heuristic, not a numeric scoring API.

High-value questions often include:

- `Is this a daily-use student tool or a presentation/marketing experience?`
- `Is mobile the primary environment or must desktop be equally first-class?`
- `Should the product feel calm/trustworthy, playful/friendly, or bold/experimental?`
- `Is fast information scanning more important than visual impact?`

Low-value questions usually include:

- exact border radius before the system exists;
- exact shadow blur;
- whether spacing should be 20px or 24px;
- a font name the user has never expressed a preference about;
- a design-style label when an experiential question would resolve the same decision better.

Infer low-value details from repository evidence and the committed direction.

## 3. Adaptive question budget

Do not use a fixed questionnaire.

Use these as soft defaults:

| Ambiguity | Typical condition | Default interaction |
|---|---|---|
| very low | purpose, audience, system and tone already clear | 0 questions; proceed |
| low | one consequential detail remains | 0-1 targeted question |
| medium | direction is known but product/tone/platform has a fork | 1-3 questions |
| high | new surface with unclear purpose/audience/design direction | 2-4 questions |
| very high | request is essentially `make a cool website` | 3-5 high-value questions, preferably grouped once |

These are not quotas. Ask fewer when one answer collapses several uncertainties. Ask more only when separate high-impact decisions remain unresolved.

Never turn discovery into an interview loop. Prefer one compact batch over serially asking one small question at a time.

## 4. Ask in user language, not taxonomy language

Prefer experiential contrasts:

```text
정보를 빨리 훑는 느낌이 중요해, 아니면 첫인상이 강한 게 중요해?

차분하고 고급스러움 / 친근하고 부드러움 / 실험적이고 강렬함 중 어디에 더 가까워?

버튼이나 카드가 눌리고 따라오는 손맛이 필요해, 아니면 움직임은 최소화할까?

앱처럼 빽빽하고 빠르게 쓸 화면이야, 아니면 여백과 이미지가 중요한 소개 페이지야?
```

Avoid questions like:

```text
neumorphism vs glassmorphism?
brutalism or neo-brutalism?
8px or 12px radius?
```

unless the user already uses those terms or the distinction itself is what they are choosing.

## 5. Use candidate directions when vocabulary is the blocker

When the user can recognize a direction more easily than describe one, offer **2-3 materially different direction cards** rather than many token questions.

Example:

```text
A. Calm Utility
   information-first, restrained color, compact motion

B. Soft Tactile
   softer surfaces, subtle translucency, press feedback, spring microinteraction

C. Editorial Bold
   strong typography, asymmetric composition, higher visual contrast
```

Each candidate should differ in meaningful product experience, not just palette.

Do not offer candidates when repository design rules already determine the answer.

## 6. Respect `you decide`

If the user says `알아서`, `you decide`, `pick what fits`, or equivalent:

1. stop asking optional design questions;
2. choose from product/repository evidence;
3. record the assumption in the Design Read when it matters;
4. keep the choice reversible where practical;
5. proceed.

Do not repeatedly seek confirmation after the user delegated the decision.

## 7. Resolve contradictions, not harmless uncertainty

Ask when two authoritative signals conflict, for example:

- user asks for playful maximalism but repository brand rules require restrained institutional UI;
- mobile-first direct manipulation conflicts with a desktop-only accessibility requirement;
- the user asks to copy a reference exactly while repository rules prohibit that visual language.

Do not ask merely because several equally acceptable micro-decisions exist.

## 8. Translate answers into Design DNA

After discovery, represent the chosen direction as a combination of axes rather than one style label.

Use [design-taxonomy.md](design-taxonomy.md).

A compact Design DNA may look like:

```text
philosophy: soft minimal + functional
material: restrained translucency
composition: asymmetric compact grid
interaction: tactile microinteraction
motion: short spring, interruptible where manipulated
density: balanced-compact
type: neutral body + characterful display only where useful
color: quiet neutral base + one restrained accent
signature: context-aware timetable focus state
```

The user does not need to see this internal representation unless it helps them choose or review.

## 9. Entry and exit conditions

Enter adaptive discovery when:

- a substantial user-facing UI is new or materially redesigned; and
- one or more high-impact design/product decisions are genuinely unresolved.

Exit discovery when:

- the primary user job and surface are clear enough to build;
- the design direction is coherent enough to constrain implementation;
- remaining unknowns are low-cost/reversible or delegated to the agent.

Do not block implementation waiting for perfect certainty.

## 10. Continuity

When Sloar durable continuity is active, keep only the committed Design DNA and unresolved high-impact decisions in hot state. Do not checkpoint the entire question transcript.

Repository-owned design documents, tokens, accepted screenshots, and component implementations remain more authoritative than recovered chat phrasing.