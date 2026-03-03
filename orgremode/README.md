# orgremode:  Org Mode for Agents

org mode is for planning.  orgremode is a variant of orgmode that implements a subset of the spec.

You have access to an agent that specializes in reading, writing, and maintaining valid org mode
files via the `orgremode` command.  It provides a --oneshot flag for agents and terse humans.

## Quick Start

You can quickly consult the plan (or update the plan) with --oneshot mode:

```console
orgremode --oneshot "What's next?" plan.org
```

## Filesystem Integration

Imagine you keep a plan.org at the root of your repo.

You could do the following:

```console
orgremode --oneshot "Capture TODO statements in the code in the plan." plan.org .
```

Notice the second argument gives the current directory.  It will be read-only.

The agent will read over `.` as best it can and look for TODO statements.

## Non-Skill Information

The prefix of the file up to this section doubles as an agent skill.  You can access the version of
the skill distributed with your `orgremode` install by invoking `orgremode --skill`.

```console
cargo install orgremode
mkdir -p .claude/skills/orgremode-oneshot \
&& orgremode --skill > .claude/skills/orgremode-oneshot/SKILL.md
```
