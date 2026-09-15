# hello

The smallest possible Vela project, and the fixture the early milestones test against.

It is byte-for-byte what `vela new hello` produces, which is deliberate: a fixture that
can drift from the scaffolder stops being a test of the scaffolder.

```vela
label start:
    "Hello, world."
    return
```

By M11, `vela check`, `vela run --headless`, and `vela test` all run against this
directory.
