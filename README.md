### `wkit` - whs31's project toolkit

#### What is `wkit`?
`wkit` is one CLI that replaces the Justfiles and scripts copied into every C, C++ and Rust repository of mine and my team. It versions, formats, checks, builds, tests, packages and installs a project described by a small `wkit.toml`, and keeps multi-repo workspaces in sync. It drives Conan, CMake and Cargo underneath.

```bash
cargo install wkit
wkit init # writes wkit.toml from what the project contains
```

#### Examples
##### Show version information

```bash
wkit bump
```

Output:
```bash
conanfile.py               2.4.1
CMakeLists.txt             2.4.1
packaging/arch/PKGBUILD    2.4.1
include/geodesy/version.h  2.4.1
```

##### Bump a version
```bash
wkit bump --patch --dry-run # or -p, -m, -M, --assign 1.2.3, --pre rc
```

Output:
```bash
     Bumping geodesy 2.4.1 -> 2.4.2
    Updating conanfile.py (2.4.1 -> 2.4.2)
    Updating CMakeLists.txt (2.4.1 -> 2.4.2)
    Updating packaging/arch/PKGBUILD (2.4.1 -> 2.4.2)
    Updating include/geodesy/version.h (2.4.1 -> 2.4.2)
warning: aborting bump due to dry run
```

##### Run checks
```bash
wkit check --only format,versions,secrets,whitespace
```

Output:
```bash
    Checking 5 C/C++ files with clang-format 23.1.3
    Checking versions (4 files)
    Checking 14 files for secrets
    Checking whitespace in uncommitted changes
          ok format      formatted       0.06s
          ok versions    all at 2.4.1    0.00s
          ok secrets     14 files clean  0.00s
          ok whitespace  clean           0.00s
    Finished checks in 0.07s
```

##### Build, test and ship
```bash
wkit fmt                # rustfmt, clang-format, qmlformat, black, stylua
wkit build --release    # conan, cmake or cargo
wkit test               # cargo, nextest, ctest, catch2
wkit run -- --help      # build a [[bin]] and run it
wkit package deb        # or arch, windows
wkit install --dry-run  # apt, pacman or plain files
```

##### Sync a multi-repo workspace
```bash
wkit ws init
wkit ws status
```

Output:
```bash
      Cloned support/core  main @ 3746af5f
      Cloned protocol/link  main @ 4230db27
    Finished 2 cloned in 0.02s
     Locking support/core  -------- → 3746af5f
     Locking protocol/link  -------- → 4230db27
      Locked 2 of 2 projects changed in workspace.lock

3746af5f  main  support/core  dirty
4230db27  main  protocol/link
```

Run `wkit doctor` to see which tools a project needs, and `wkit --help` for everything else.

#### Coding agents
[`skills/wkit/SKILL.md`](skills/wkit/SKILL.md) teaches coding agents to use `wkit` instead of calling formatters, linters and build tools directly. Copy it to `.claude/skills/wkit/` or `.agents/skills/wkit/` of a project that has a `wkit.toml`.
