# Translating hrtz

Each language is one file here:

| File | Language |
|---|---|
| `de.json` | Deutsch |
| `es.json` | Español |
| `pt-BR.json` | Português (Brasil) |
| `ru.json` | Русский |
| `uk.json` | Українська |
| `zh-CN.json` | 简体中文 |

`en.json` is the English, made from the app's code. The first versions were machine-drafted, so the most useful help is someone who speaks the language reading them in the app and fixing what sounds wrong.

## How to translate

Open your language's file. Each line is the English on the left and your language on the right:

```json
"Polling rate": "Частота опитування",
```

Change the right side. A line left empty shows in English, so new lines can be translated a few at a time.

To add a language, copy a file's keys with empty values, name it by the language's code, and ask us to add it to the list in `src/i18n.ts`.

## Keep these as they are

- **`{{names}}` in double braces** are filled in by the app: `"At {{dpi}} DPI"` becomes "At 1,600 DPI". Keep each one, spelled the same. You can move it within the sentence.
- **`<1>…</1>` tags** wrap a link or bold text. Keep the tags around the matching words.
- **Names**: hrtz, the mouse names (Z1 PRO MAX, G PRO X SUPERLIGHT), Zaopin, Logitech, G HUB, SignalRGB, Windows.

## Counts

A line with a count has one entry per plural form your language has. German has `_one` and `_other`; Spanish and Portuguese add `_many`, used only for millions in compact form (the same text as `_other`); Chinese has `_other` alone. Ukrainian and Russian have four:

```json
"{{count}} presses_one": "{{count}} натискання",
"{{count}} presses_few": "{{count}} натискання",
"{{count}} presses_many": "{{count}} натискань",
"{{count}} presses_other": "{{count}} натискання",
```

- `_one`: 1, 21, 31, …
- `_few`: 2–4, 22–24, …
- `_many`: 5–20, 25–30, …
- `_other`: fractions (1.5)

## The same word in different places

Some English words mean different things in different places. Those have an ending that says which:

- `Left_button`: the mouse button ("Left").
- `Left_side`: a side of the screen, where the HUD sits.
- `Left_direction`: a direction, as in scrolling left.
- `Right_answer`: "Right!" as in correct, in the blind test.

## What stays English

The report from Inspect a mouse, which comes to us on GitHub, and What's new.

## Trying it

In hrtz, Settings → Language. New text in the app shows in English until it's added here.
