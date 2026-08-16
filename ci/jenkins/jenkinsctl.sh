#!/usr/bin/env bash
# Drive the local Jenkins without opening a browser.
#
#   ci/jenkins/jenkinsctl.sh up      build the images, start Jenkins, create the job
#   ci/jenkins/jenkinsctl.sh build   trigger a build and wait for the verdict
#   ci/jenkins/jenkinsctl.sh status  the last build's result
#   ci/jenkins/jenkinsctl.sh log     the last build's console output
#   ci/jenkins/jenkinsctl.sh open    print the URL to open in a browser
#   ci/jenkins/jenkinsctl.sh down    stop and remove the container
#   ci/jenkins/jenkinsctl.sh clean   also delete the build cache and history
#
# The job builds what is **committed** on your current branch: it clones from
# the bind-mounted repository, so unpushed commits are fine and uncommitted
# changes are invisible. Commit first, then build.
set -euo pipefail

cd "$(dirname "$0")"
REPO="$(cd ../.. && pwd)"

JENKINS_URL=http://127.0.0.1:8080
JENKINS_HOME_DIR="${JENKINS_HOME_DIR:-$HOME/jenkins-mcopy/jenkins_home}"
AUTH="admin:admin"
JOB=mcopy-ci
COOKIES="$(mktemp -u /tmp/jenkinsctl-cookies.XXXXXX)"

# curl's URL globbing eats the square brackets in Jenkins' `tree=` queries, so
# every call here needs -g.
api() { curl -sg --user "$AUTH" "$@"; }

die() { echo "error: $*" >&2; exit 1; }

require_docker() {
    docker info >/dev/null 2>&1 || die "cannot talk to docker.
  You are probably not in the docker group yet:
      sudo usermod -aG docker \$USER    # then log out and back in"
}

# Jenkins ties the CSRF crumb to the session that asked for it, so the cookie
# jar has to be carried from the crumb request into the POST.
post() {
    local path="$1"; shift   # the rest are extra curl arguments, not URLs
    rm -f "$COOKIES"
    local crumb
    crumb="$(curl -sg -c "$COOKIES" --user "$AUTH" \
        "$JENKINS_URL/crumbIssuer/api/xml?xpath=concat(//crumbRequestField,\":\",//crumb)")"
    curl -sg -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST \
        --user "$AUTH" -H "$crumb" "$@" "$JENKINS_URL$path"
}

wait_until_up() {
    for _ in $(seq 1 60); do
        [ "$(curl -s -o /dev/null -w '%{http_code}' "$JENKINS_URL/login" || true)" = "200" ] \
            && return 0
        sleep 5
    done
    die "Jenkins did not come up. Try: docker logs jenkins | tail -50"
}

cmd_up() {
    require_docker
    echo "▸ building images"
    docker build -q -t mcopy-ci:latest      -f Dockerfile.ci      . >/dev/null
    docker build -q -t mcopy-jenkins:latest -f Dockerfile.jenkins . >/dev/null

    echo "▸ starting Jenkins"
    mkdir -p "$JENKINS_HOME_DIR"
    docker rm -f jenkins >/dev/null 2>&1 || true
    # Bound to 127.0.0.1 on purpose: Jenkins runs arbitrary code by design and
    # has no business being reachable from the network.
    #
    # JENKINS_HOME is mounted at the same path inside and out because the build
    # runs in a sibling container, and the docker daemon resolves that
    # container's workspace mount against host paths.
    docker run -d --name jenkins \
        -p 127.0.0.1:8080:8080 \
        -v "$JENKINS_HOME_DIR:$JENKINS_HOME_DIR" \
        -e JENKINS_HOME="$JENKINS_HOME_DIR" \
        -e JENKINS_ADMIN_PASSWORD=admin \
        -v "$REPO:/repo:ro" \
        -v /var/run/docker.sock:/var/run/docker.sock \
        --group-add "$(stat -c '%g' /var/run/docker.sock)" \
        mcopy-jenkins:latest >/dev/null
    wait_until_up

    if api "$JENKINS_URL/api/json?tree=jobs[name]" | grep -q "\"$JOB\""; then
        echo "▸ job $JOB already exists"
    else
        echo "▸ creating job $JOB"
        local code
        code="$(rm -f "$COOKIES"
            crumb="$(curl -sg -c "$COOKIES" --user "$AUTH" \
                "$JENKINS_URL/crumbIssuer/api/xml?xpath=concat(//crumbRequestField,\":\",//crumb)")"
            curl -sg -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST \
                --user "$AUTH" -H "$crumb" -H 'Content-Type: application/xml' \
                --data-binary @job-config.xml \
                "$JENKINS_URL/createItem?name=$JOB")"
        [ "$code" = "200" ] || die "creating the job failed (HTTP $code)"
    fi

    echo
    echo "Jenkins is up: $JENKINS_URL  (admin / admin)"
    echo "Run a build:   $0 build"
}

cmd_build() {
    require_docker
    local before
    before="$(api "$JENKINS_URL/job/$JOB/api/json?tree=nextBuildNumber" \
        | grep -o '[0-9]\+' | head -1)"
    [ -n "$before" ] || die "job $JOB not found — run: $0 up"

    local code
    code="$(post "/job/$JOB/build")"
    [ "$code" = "201" ] || die "triggering the build failed (HTTP $code)"
    echo "▸ build #$before queued"

    for i in $(seq 1 180); do
        local state
        state="$(api "$JENKINS_URL/job/$JOB/$before/api/json?tree=result,building" 2>/dev/null || true)"
        case "$state" in
            *'"building":false'*)
                local result
                result="$(echo "$state" | grep -o '"result":"[A-Z]*"' | cut -d'"' -f4)"
                echo
                if [ "$result" = "SUCCESS" ]; then
                    echo "✓ build #$before: SUCCESS"
                    echo "  Windows and macOS are still only covered by GitHub."
                    return 0
                fi
                echo "✗ build #$before: $result"
                echo "  Full log: $0 log"
                api "$JENKINS_URL/job/$JOB/$before/consoleText" \
                    | sed 's/\x1b\[[0-9;]*m//g' \
                    | grep -E '^\[.*Z\] error|^ERROR|skipped due to' | head -10
                return 1
                ;;
            *'"building":true'*) [ $((i % 6)) -eq 0 ] && printf '.' ;;
        esac
        sleep 10
    done
    die "build did not finish within 30 minutes"
}

cmd_status() {
    api "$JENKINS_URL/job/$JOB/lastBuild/api/json?tree=number,result,building,duration"
    echo
}

cmd_log() {
    api "$JENKINS_URL/job/$JOB/lastBuild/consoleText" | sed 's/\x1b\[[0-9;]*m//g'
}

cmd_open() { echo "$JENKINS_URL/job/$JOB  (admin / admin)"; }

cmd_down() {
    require_docker
    docker rm -f jenkins >/dev/null 2>&1 && echo "stopped" || echo "not running"
}

cmd_clean() {
    cmd_down
    rm -rf "$JENKINS_HOME_DIR"
    docker volume rm mcopy-target mcopy-cargo-registry >/dev/null 2>&1 || true
    echo "history and build cache removed"
}

case "${1:-}" in
    up)     cmd_up ;;
    build)  cmd_build ;;
    status) cmd_status ;;
    log)    cmd_log ;;
    open)   cmd_open ;;
    down)   cmd_down ;;
    clean)  cmd_clean ;;
    *) sed -n '2,17p' "$0"; exit 1 ;;
esac
