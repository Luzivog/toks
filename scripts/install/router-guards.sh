#!/usr/bin/env bash

toks_systemctl_present() {
    local toks_probe_output
    local toks_probe_status
    if toks_probe_output=$(timeout --signal=KILL 2s systemctl --user show \
        --property=LoadState "$1" 2>&1); then
        toks_probe_status=0
    else
        toks_probe_status=$?
    fi
    if ((toks_probe_status != 0)); then
        echo "could not determine systemd user-manager state for $1: $toks_probe_output" >&2
        return 2
    fi
    case "$toks_probe_output" in
        LoadState=not-found) return 1 ;;
        LoadState=?*) return 0 ;;
        *)
            echo "systemd returned an indeterminate state for $1: $toks_probe_output" >&2
            return 2
            ;;
    esac
}

toks_prepare_directory_under() {
    local toks_directory_root=$1
    local toks_relative=$2
    local toks_current=
    local toks_component
    local -a toks_root_components toks_components
    [[ "$toks_directory_root" == /* ]] || {
        echo "managed install root is not absolute: $toks_directory_root" >&2
        return 1
    }
    IFS=/ read -r -a toks_root_components <<<"$toks_directory_root"
    for toks_component in "${toks_root_components[@]}"; do
        [[ -z "$toks_component" ]] && continue
        [[ "$toks_component" != . && "$toks_component" != .. ]] || {
            echo "invalid managed install root: $toks_directory_root" >&2
            return 1
        }
        toks_current="$toks_current/$toks_component"
        if [[ -L "$toks_current" ]]; then
            echo "managed install root ancestor is a symlink: $toks_current" >&2
            return 1
        fi
        if [[ -e "$toks_current" && ! -d "$toks_current" ]]; then
            echo "managed install root ancestor is not a directory: $toks_current" >&2
            return 1
        fi
        [[ -d "$toks_current" ]] || mkdir "$toks_current"
    done
    toks_current=$toks_directory_root
    IFS=/ read -r -a toks_components <<<"$toks_relative"
    for toks_component in "${toks_components[@]}"; do
        [[ -n "$toks_component" && "$toks_component" != . && "$toks_component" != .. ]] || {
            echo "invalid managed install directory: $toks_relative" >&2
            return 1
        }
        toks_current="$toks_current/$toks_component"
        if [[ -L "$toks_current" ]]; then
            echo "managed install directory is a symlink: $toks_current" >&2
            return 1
        fi
        if [[ -e "$toks_current" && ! -d "$toks_current" ]]; then
            echo "managed install directory is not a directory: $toks_current" >&2
            return 1
        fi
        [[ -d "$toks_current" ]] || mkdir "$toks_current"
    done
}
