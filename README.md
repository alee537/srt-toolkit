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
  sequential numbering, zero-padded timestamps, and LF line endings.

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

SubRip (`.srt`) only for now. The parser produces a format-agnostic `Cue`
list internally, so adding WebVTT input later means writing another
parser, not touching the validator or formatter.
