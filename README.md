# srt-toolkit

Every few months I end up with a `.srt` file - ripped off an old disc,
exported from some transcription service, hand-edited by someone in a text
editor with the wrong line-ending settings - that plays fine in one player
and silently breaks in another. Usually it's something small: timestamps
that go backwards, a cue with a timing line but no text, two cues that
overlap by half a second. This is a command-line tool to catch that before
it becomes someone else's problem, plus a formatter that rewrites a file
into one consistent shape.

Two commands:

- `validate` - parses the file and reports structural parse errors (a cue
  number that isn't a number, a timing line missing `-->`, a cue with no
  text) separately from semantic issues (overlapping cues, a cue that ends
  before it starts, duplicate cue numbers).
- `format` - reparses a structurally valid file and rewrites it with
  sequential numbering, zero-padded timestamps, and LF line endings. With
  `--fix`, it also corrects the two timing issues `validate` can flag:
  a cue that starts before the previous one ends gets its start time
  pushed to the previous cue's end, and a cue whose end isn't after its
  start gets its end time pushed one millisecond past its start. Fixes
  are listed on stderr (or in a `fixes` array with `--json`) so the
  reformatted file itself stays clean to redirect or pipe. Duplicate cue
  numbers aren't something `--fix` touches - `format` always renumbers
  sequentially, so there's nothing left to fix by the time it runs.

Both commands accept `.srt` or `.vtt` input - the format is picked from the
file extension, and either way you get the same `validate`/`format`
behavior since both parsers produce the same internal cue list. `format`
always writes SubRip; there's no WebVTT output yet.

Both commands take `--json` if you want to hand the result to another
program instead of reading it yourself.

## build

```
cargo build --release
```

No third-party crates, just the standard library.

## usage

Given `movie.srt`:

```
1
00:00:01,000 --> 00:00:04,000
Hello there.

2
00:00:03,500 --> 00:00:06,000
This overlaps the previous cue.
```

```
$ srt-toolkit validate movie.srt
movie.srt: 0 error(s), 1 issue(s)
  cue 2: starts before the previous cue ends

$ srt-toolkit validate movie.srt --json
{
  "file": "movie.srt",
  "valid": false,
  "cue_count": 2,
  "errors": [
  ],
  "issues": [
    { "cue": 2, "message": "starts before the previous cue ends" }
  ]
}

$ srt-toolkit format movie.srt
1
00:00:01,000 --> 00:00:04,000
Hello there.

2
00:00:03,500 --> 00:00:06,000
This overlaps the previous cue.
```

`format` refuses to run if the file has structural parse errors - fix
those first (`validate` will point at the exact line) since renumbering a
file that couldn't be parsed correctly would just produce a different kind
of garbage.

## why not just use ffmpeg or an existing subtitle library

Those convert between formats fine, but they don't tell you *why* a file
is broken, and I wanted something small enough to drop into a pre-commit
hook or a CI step without pulling in a runtime dependency.

## status

Reads SubRip (`.srt`) and WebVTT (`.vtt`) input; writes SubRip only. Cue
identifiers from WebVTT aren't preserved - like SubRip cue numbers, output
numbering is always sequential, so there was nothing format-specific worth
carrying through the internal `Cue` list.
