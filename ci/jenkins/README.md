# Local CI with Jenkins

A Jenkins instance in Docker that runs the same checks as
`.github/workflows/ci.yml`, so a broken build is caught here instead of ten
minutes into a GitHub run.

**What it does not cover:** Windows and macOS. GitHub runs the `check` job on
all three platforms; Docker can only host Linux, and this repository has real
`#[cfg(target_os)]` code behind the other two. A green build here means *the
Linux job will pass* — nothing more.

For most edits `../../scripts/preflight.sh` is the better tool: same Linux
coverage, three seconds, no containers. Jenkins earns its place when you want
the packaging job exercised in a clean environment, or a build history to look
back at.

## Contents

| File | Purpose |
| --- | --- |
| `jenkinsctl.sh` | Start it, run builds, read results — the only entry point you need |
| `Dockerfile.ci` | Build agent: Rust plus everything gpui links against |
| `Dockerfile.jenkins` | Jenkins plus a docker CLI, plugins preinstalled, no setup wizard |
| `jenkins.yaml` | The entire Jenkins configuration (JCasC) |
| `job-config.xml` | The job definition |
| `plugins.txt` | Plugins baked into the image |
| `../../Jenkinsfile` | The pipeline itself |

Nothing is configured by clicking. Delete the container and rebuild it and you
get the same instance back.

## Using it

You need to be in the `docker` group first:

```bash
sudo usermod -aG docker $USER      # then log out and back in
docker info                        # must succeed without sudo
```

Everything else goes through one script:

```bash
./ci/jenkins/jenkinsctl.sh up       # build images, start Jenkins, create the job
./ci/jenkins/jenkinsctl.sh build    # trigger a build, wait, print the verdict
./ci/jenkins/jenkinsctl.sh status   # last build's result
./ci/jenkins/jenkinsctl.sh log      # last build's console output
./ci/jenkins/jenkinsctl.sh down     # stop
./ci/jenkins/jenkinsctl.sh clean    # stop and forget history and build cache
```

`up` is idempotent — run it again after editing a `Dockerfile` and it rebuilds
and restarts without losing the job.

Or use the web UI at <http://127.0.0.1:8080> (`admin` / `admin`) and press
**Build Now**.

### The one rule

The job builds what is **committed** on your branch. It clones from the
bind-mounted repository, so unpushed commits are fine — uncommitted changes are
invisible. Commit first, then build.

### How long it takes

Measured on this repository:

| | Time |
| --- | --- |
| First build (cold cache) | ~6 minutes |
| Later builds | ~40 seconds |

The difference is `mcopy-target` and `mcopy-cargo-registry`, two named volumes
holding `CARGO_TARGET_DIR` and cargo's registry. They outlive the workspace,
which Jenkins wipes between builds. `clean` deletes them, so the next build is
a cold one again.

For comparison: `scripts/preflight.sh` covers the same ground minus packaging
in about three seconds, because it reuses the `target/` you already have.

## Why the container is started the way it is

Three flags in `jenkinsctl.sh` are load-bearing, and each one cost a failed
build to discover:

- **`JENKINS_HOME` is bind-mounted at the same path inside and outside the
  container.** The build runs in a *sibling* container started through the
  mounted docker socket, and the daemon resolves that container's workspace
  mount against host paths. A different path inside would give the build an
  empty workspace.
- **The port is bound to `127.0.0.1`, not `0.0.0.0`.** Jenkins runs arbitrary
  code by design and should not be reachable from the network.
- **`ALLOW_LOCAL_CHECKOUT` is enabled** (in `Dockerfile.jenkins`). The git
  plugin refuses to clone from a local directory by default, because on a
  shared Jenkins that would let any job read arbitrary host paths. Here the
  local clone is the entire point, the repository is mounted read-only, and the
  instance is single-user on localhost.

Two more things the agent image has to provide, for the same reason:

- The cargo cache directories must **exist and be writable in the image**.
  Docker seeds a named volume from the image, ownership included, and only
  ever does so once — so a directory created later comes up owned by root and
  cargo cannot write to its own registry.
- The agent runs as a uid with no passwd entry, so it needs a **`HOME` that
  exists**: the shell-integration tests register menu entries under the user's
  own directories, which is precisely what they assert.

## When something breaks

```bash
docker logs jenkins | tail -50
```

A configuration mistake in `jenkins.yaml` makes Jenkins refuse to start, and
the error is a `ConfigurationAsCodeBootFailure` naming the offending key. Do
not run the container with `--restart unless-stopped` while you are changing
that file: it turns a single clear failure into a crash loop.
