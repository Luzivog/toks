#!/usr/bin/env bash
set -euo pipefail

toks_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
toks_sandbox=$(mktemp -d)
trap 'rm -rf "$toks_sandbox"' EXIT

toks_fixture() {
    local fixture=$1
    mkdir -p \
        "$fixture/target/release" \
        "$fixture/assets" \
        "$fixture/mock-bin" \
        "$fixture/scripts/install"
    cp "$toks_root/install.sh" "$fixture/install.sh"
    cp "$toks_root/scripts/install/router-guards.sh" "$fixture/scripts/install/"
    cp "$toks_root/assets/toks.desktop" "$toks_root/assets/toks.svg" "$fixture/assets/"
    printf '#!/usr/bin/env bash\nexit 0\n' >"$fixture/target/release/toks"
    cat >"$fixture/target/release/toks-router" <<'EOF'
#!/usr/bin/env bash
if [[ ${1:-} == install-service ]]; then
    printf 'install-service %s\n' "${2:-manual}" >>"$TOKS_TEST_ROUTER_LOG"
    if [[ -e "$TOKS_TEST_ENABLE_INTENT" ]]; then
        printf 'applied\n' >>"$TOKS_TEST_ROUTER_APPLIED_LOG"
    fi
fi
EOF
    cat >"$fixture/mock-bin/update-desktop-database" <<'EOF'
#!/usr/bin/env bash
if [[ ${TOKS_TEST_ENABLE_DURING_INSTALL:-0} == 1 ]]; then
    : >"$TOKS_TEST_ENABLE_INTENT"
fi
EOF
    cat >"$fixture/mock-bin/systemctl" <<'EOF'
#!/usr/bin/env bash
case ${TOKS_TEST_SYSTEMCTL_MODE:-inactive} in
    inactive)
        printf 'LoadState=not-found\n'
        exit 0
        ;;
    error) exit 42 ;;
    timeout) exec sleep 30 ;;
esac
EOF
    chmod +x \
        "$fixture/install.sh" \
        "$fixture/target/release/toks" \
        "$fixture/target/release/toks-router" \
        "$fixture/mock-bin/update-desktop-database" \
        "$fixture/mock-bin/systemctl"
}

toks_run_install() {
    local fixture=$1
    HOME="$fixture/home" \
        XDG_DATA_HOME="$fixture/data" \
        XDG_CONFIG_HOME="$fixture/config" \
        TOKS_INSTALL_PREFIX="$fixture/prefix" \
        TOKS_TEST_ROUTER_LOG="$fixture/router.log" \
        TOKS_TEST_ROUTER_APPLIED_LOG="$fixture/router-applied.log" \
        TOKS_TEST_ENABLE_INTENT="$fixture/enabled" \
        TOKS_TEST_ENABLE_DURING_INSTALL="${TOKS_TEST_ENABLE_DURING_INSTALL:-0}" \
        TOKS_TEST_SYSTEMCTL_MODE="${TOKS_TEST_SYSTEMCTL_MODE:-inactive}" \
        PATH="$fixture/mock-bin:/usr/bin:/bin" \
        "$fixture/install.sh" >/dev/null
}

toks_fresh="$toks_sandbox/fresh"
toks_fixture "$toks_fresh"
toks_run_install "$toks_fresh"
[[ $(<"$toks_fresh/router.log") == "install-service $toks_fresh/prefix/bin/toks-router" ]]
[[ ! -e "$toks_fresh/router-applied.log" ]]
toks_hash=$(sha256sum "$toks_fresh/target/release/toks-router" | awk '{print $1}')
toks_stable="$toks_fresh/data/toks/rotation/router-artifacts/executables/$toks_hash/toks-router"
[[ $(readlink "$toks_fresh/prefix/bin/toks-router") == "$toks_stable" ]]
[[ -x "$toks_stable" ]]

mkdir -p "$toks_fresh/config/systemd/user"
printf '{}\n' >"$toks_fresh/config/systemd/user/.toks-router-install-pending.json"
: >"$toks_fresh/router.log"
toks_run_install "$toks_fresh"
[[ $(<"$toks_fresh/router.log") == "install-service $toks_fresh/prefix/bin/toks-router" ]]

toks_flip="$toks_sandbox/intent-flips-after-probe"
toks_fixture "$toks_flip"
TOKS_TEST_ENABLE_DURING_INSTALL=1 toks_run_install "$toks_flip"
[[ $(<"$toks_flip/router.log") == "install-service $toks_flip/prefix/bin/toks-router" ]]
[[ $(<"$toks_flip/router-applied.log") == applied ]]

toks_units="$toks_sandbox/units"
toks_fixture "$toks_units"
mkdir -p "$toks_units/config/systemd/user"
for unit in \
    toks-router.service \
    toks-router.socket \
    toks-router-worker@.service \
    toks-router-resume.service; do
    : >"$toks_units/config/systemd/user/$unit"
done
toks_run_install "$toks_units"
[[ $(<"$toks_units/router.log") == "install-service $toks_units/prefix/bin/toks-router" ]]

toks_collision="$toks_sandbox/collision"
toks_fixture "$toks_collision"
toks_collision_hash=$(sha256sum "$toks_collision/target/release/toks-router" | awk '{print $1}')
toks_collision_artifact="$toks_collision/data/toks/rotation/router-artifacts/executables/$toks_collision_hash/toks-router"
mkdir -p "$(dirname "$toks_collision_artifact")"
printf 'different-router\n' >"$toks_collision_artifact"
if toks_run_install "$toks_collision" 2>/dev/null; then
    echo "install accepted an occupied content address" >&2
    exit 1
fi
[[ $(<"$toks_collision_artifact") == different-router ]]

toks_symlink_escape="$toks_sandbox/symlink-escape"
toks_fixture "$toks_symlink_escape"
mkdir -p "$toks_symlink_escape/data/toks/rotation/router-artifacts" \
    "$toks_symlink_escape/outside"
ln -s "$toks_symlink_escape/outside" \
    "$toks_symlink_escape/data/toks/rotation/router-artifacts/executables"
if toks_run_install "$toks_symlink_escape" 2>/dev/null; then
    echo "install followed a router artifact ancestor symlink" >&2
    exit 1
fi
[[ -z $(find "$toks_symlink_escape/outside" -mindepth 1 -print -quit) ]]

toks_parent_escape="$toks_sandbox/parent-escape"
toks_fixture "$toks_parent_escape"
mkdir -p "$toks_parent_escape/data/toks" "$toks_parent_escape/outside"
ln -s "$toks_parent_escape/outside" "$toks_parent_escape/data/toks/rotation"
if toks_run_install "$toks_parent_escape" 2>/dev/null; then
    echo "install followed a router artifact parent symlink" >&2
    exit 1
fi
[[ -z $(find "$toks_parent_escape/outside" -mindepth 1 -print -quit) ]]

toks_root_escape="$toks_sandbox/root-escape"
toks_fixture "$toks_root_escape"
mkdir -p "$toks_root_escape/outside"
ln -s "$toks_root_escape/outside" "$toks_root_escape/data"
if toks_run_install "$toks_root_escape" 2>/dev/null; then
    echo "install followed the artifact data-root symlink" >&2
    exit 1
fi
[[ -z $(find "$toks_root_escape/outside" -mindepth 1 -print -quit) ]]

for toks_probe_mode in error timeout; do
    toks_probe="$toks_sandbox/probe-$toks_probe_mode"
    toks_fixture "$toks_probe"
    if TOKS_TEST_SYSTEMCTL_MODE=$toks_probe_mode toks_run_install "$toks_probe" 2>/dev/null; then
        echo "install accepted indeterminate systemd state: $toks_probe_mode" >&2
        exit 1
    fi
    [[ ! -e "$toks_probe/prefix/bin/toks" ]]
done

toks_existing_probe="$toks_sandbox/existing-probe-error"
toks_fixture "$toks_existing_probe"
mkdir -p "$toks_existing_probe/config/systemd/user"
for toks_unit in \
    toks-router.service \
    toks-router.socket \
    toks-router-worker@.service \
    toks-router-resume.service; do
    : >"$toks_existing_probe/config/systemd/user/$toks_unit"
done
if TOKS_TEST_SYSTEMCTL_MODE=error toks_run_install "$toks_existing_probe" 2>/dev/null; then
    echo "install ignored manager failure because unit files existed" >&2
    exit 1
fi
[[ ! -e "$toks_existing_probe/prefix/bin/toks" ]]

echo "install recovery tests passed"
