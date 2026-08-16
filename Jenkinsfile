// Local CI, mirroring .github/workflows/ci.yml.
//
// The stages below are the same commands, in the same order, as that
// workflow's `check` and `packaging` jobs — with one difference that cannot be
// closed locally: GitHub runs `check` on windows-latest, macos-latest and
// ubuntu-latest, and this runs only the Linux equivalent. Docker cannot host
// the other two, so a green build here means "the Linux job will pass", not
// "CI will pass". See ci/jenkins/README.md.
//
// Build dependencies are baked into mcopy-ci:latest rather than apt-installed
// per run, and cargo's registry and target directory live in named volumes, so
// a second build reuses the first one's compilation.

pipeline {
    agent {
        docker {
            image 'mcopy-ci:latest'
            args  '-v mcopy-cargo-registry:/usr/local/cargo/registry ' +
                  '-v mcopy-target:/cargo-target'
        }
    }

    environment {
        CARGO_TERM_COLOR = 'always'
        // Outside the workspace and on a named volume: the workspace is wiped
        // between builds, and recompiling gpui every time is the one thing
        // that would make this slower than pushing.
        CARGO_TARGET_DIR = '/cargo-target'
        // Jenkins starts the agent as a uid with no passwd entry. The
        // shell-integration tests write to the user's own directories, so
        // they need a home that exists; the image provides this one.
        HOME = '/home/ci'
    }

    options {
        timestamps()
        // A hung build must not hold the executor forever.
        timeout(time: 40, unit: 'MINUTES')
    }

    stages {
        // ---- .github/workflows/ci.yml :: check ------------------------------
        stage('Format') {
            steps { sh 'cargo fmt --all --check' }
        }

        stage('Lint') {
            steps { sh 'cargo clippy --all-targets --locked -- -D warnings' }
        }

        stage('Test') {
            steps { sh 'cargo test --locked' }
        }

        stage('CLI smoke test') {
            // The GUI needs a desktop session; the CLI paths do not. This
            // catches a binary that fails to start at all.
            steps {
                sh '''
                    cargo run --locked --release -- --version
                    cargo run --locked --release -- status
                '''
            }
        }

        // ---- .github/workflows/ci.yml :: packaging -------------------------
        stage('Package') {
            steps { sh './scripts/package-linux.sh' }
        }

        stage('Verify the AppImage runs') {
            // Same assertion as the CI step. --appimage-extract-and-run avoids
            // needing FUSE, which a container does not have.
            steps {
                sh '''
                    appimage="$(ls -t dist/mcopy-*-x86_64.AppImage | head -1)"
                    chmod +x "$appimage"
                    "$appimage" --appimage-extract-and-run --version
                    "$appimage" --appimage-extract-and-run status
                '''
            }
        }
    }

    post {
        success {
            archiveArtifacts artifacts: 'dist/*',
                             fingerprint: true,
                             allowEmptyArchive: true
        }
        always {
            // package-linux.sh writes into the workspace; leaving the tree
            // dirty would make the next build's git checkout noisy.
            sh 'rm -rf dist || true'
        }
    }
}
