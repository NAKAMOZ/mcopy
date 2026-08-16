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
| `Dockerfile.ci` | Build agent: Rust plus everything gpui links against |
| `Dockerfile.jenkins` | Jenkins plus a docker CLI, plugins preinstalled, no setup wizard |
| `jenkins.yaml` | The entire Jenkins configuration (JCasC) |
| `plugins.txt` | Plugins baked into the image |
| `../../Jenkinsfile` | The pipeline itself |

Nothing is configured by clicking. Delete the container and rebuild it and you
get the same instance back.

## One-time setup

You need to be in the `docker` group:

```bash
sudo usermod -aG docker $USER      # then log out and back in
docker info                        # must succeed without sudo
```

Build both images:

```bash
cd ci/jenkins
docker build -t mcopy-ci:latest      -f Dockerfile.ci      .
docker build -t mcopy-jenkins:latest -f Dockerfile.jenkins .
```

## Starting it

```bash
JH=$HOME/jenkins-mcopy/jenkins_home
mkdir -p "$JH"
docker run -d --name jenkins \
  -p 127.0.0.1:8080:8080 \
  -v "$JH:$JH" -e JENKINS_HOME="$JH" \
  -e JENKINS_ADMIN_PASSWORD=admin \
  -v "$PWD/../..:/repo:ro" \
  -v /var/run/docker.sock:/var/run/docker.sock \
  --group-add "$(stat -c '%g' /var/run/docker.sock)" \
  mcopy-jenkins:latest
```

Then open <http://127.0.0.1:8080> and log in as `admin` / `admin`.

Two details in that command are load-bearing:

- **`JENKINS_HOME` is bind-mounted at the same path inside and outside the
  container.** The pipeline runs its build in a *sibling* container started
  through the mounted docker socket, and the daemon resolves the workspace
  mount against host paths. If the path differed, the build container would
  come up with an empty workspace.
- **The port is bound to `127.0.0.1`, not `0.0.0.0`.** Jenkins can run
  arbitrary code by design; this one should not be reachable from the network.

## Running a build

The job builds whatever is **committed** on `main` in your working copy — it
clones from the bind-mounted repository, so unpushed commits are fine, but
uncommitted changes are not seen. Commit first, then build.

From the UI: open **mcopy-ci** and press **Build Now**.

From the terminal:

```bash
# Trigger
crumb=$(curl -s --user admin:admin \
  'http://127.0.0.1:8080/crumbIssuer/api/xml?xpath=concat(//crumbRequestField,":",//crumb)')
curl -s -X POST --user admin:admin -H "$crumb" \
  http://127.0.0.1:8080/job/mcopy-ci/build

# Watch the log of the newest build
curl -s --user admin:admin \
  http://127.0.0.1:8080/job/mcopy-ci/lastBuild/consoleText

# Just the verdict
curl -s --user admin:admin \
  'http://127.0.0.1:8080/job/mcopy-ci/lastBuild/api/json?tree=number,result,building'
```

The first build compiles gpui from scratch and takes a while. Later builds
reuse it: `CARGO_TARGET_DIR` and cargo's registry live in named volumes
(`mcopy-target`, `mcopy-cargo-registry`) that outlive the workspace.

## Stopping and cleaning up

```bash
docker stop jenkins                     # keeps history
docker rm -f jenkins                    # removes the container, keeps $JH
rm -rf ~/jenkins-mcopy/jenkins_home     # forget everything
docker volume rm mcopy-target mcopy-cargo-registry   # drop the build cache
```

## When something breaks

```bash
docker logs jenkins | tail -50
```

A configuration mistake in `jenkins.yaml` makes Jenkins refuse to start, and
the error is a `ConfigurationAsCodeBootFailure` naming the offending key. Do
not run the container with `--restart unless-stopped` while you are changing
that file: it turns a single clear failure into a crash loop.
