"""Pure contracts for the exact daemon-observed isolated container configuration."""
import hashlib
import json
import os
import re

from journey_process import require
from journey_reliability_transport import CONTAINER_ID, DIGEST, IMAGE_REF
from journey_reliability_seccomp import security_options_contract
from journey_reliability_native_runtime import selected_runtime

OWNER_LABEL = "org.temet.mike.release-runner-owner"
CONTRACT_LABEL = "org.temet.mike.release-runner-contract"
CONTRACT_VERSION = "normal-default-v1"
UID = os.geteuid() or 65532
GID = os.getegid() if os.geteuid() else 65532
MEMORY_BYTES = 4 * 1024 ** 3
TMPFS_BYTES = 3 * 1024 ** 3
TMPFS = f"rw,nosuid,nodev,exec,size={TMPFS_BYTES},uid={UID},gid={GID},mode=0700"
ENVIRONMENT = {"PATH": "/usr/lib/postgresql/19/bin:/usr/bin:/bin", "HOME": "/work/home",
               "TMPDIR": "/work/tmp", "LC_ALL": "C", "LANG": "C", "PYTHONDONTWRITEBYTECODE": "1"}


def image_contract(value, image):
    require(isinstance(image, str) and IMAGE_REF.fullmatch(image), "ISOLATED_IMAGE_DIGEST_REQUIRED")
    require(value.get("Os") == "linux" and value.get("Architecture") == "amd64"
            and value.get("Variant", "") in ("", None), "ISOLATED_IMAGE_PLATFORM")
    require(isinstance(value.get("Id"), str) and DIGEST.fullmatch(value["Id"])
            and image in (value.get("RepoDigests") or []), "ISOLATED_IMAGE_IDENTITY")
    config = value.get("Config")
    require(isinstance(config, dict) and not config.get("Volumes"), "ISOLATED_IMAGE_VOLUMES_FORBIDDEN")
    env = config.get("Env") or []
    require(isinstance(env, list) and all(isinstance(item, str) and "=" in item for item in env),
            "ISOLATED_IMAGE_ENVIRONMENT")
    keys = [item.split("=", 1)[0] for item in env]
    require(len(keys) == len(set(keys)) and set(keys) <= set(ENVIRONMENT), "ISOLATED_IMAGE_ENVIRONMENT")
    return value["Id"]


def create_arguments(image_id, name, owner, bootstrap, payload_sha256, seccomp_path, runtime_name):
    selected_runtime(runtime_name)
    args = ["container", "create", "--pull=never", "--name", name,
            "--label", OWNER_LABEL + "=" + owner,
            "--label", CONTRACT_LABEL + "=" + CONTRACT_VERSION,
            "--platform", "linux/amd64", "--runtime", runtime_name, "--network", "none", "--user", f"{UID}:{GID}",
            "--read-only", "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
            "--security-opt", "seccomp=" + str(seccomp_path), "--pids-limit", "256", "--memory", str(MEMORY_BYTES), "--memory-swap", str(MEMORY_BYTES),
            "--cpus", "2", "--shm-size", str(64 * 1024 ** 2), "--ipc", "private", "--cgroupns", "private",
            "--restart", "no", "--no-healthcheck", "--log-driver", "none",
            "--tmpfs", "/work:" + TMPFS, "--workdir", "/work", "--hostname", "mike-release-runner",
            "--entrypoint", "/usr/bin/python3"]
    for key, value in ENVIRONMENT.items():
        args.extend(("--env", key + "=" + value))
    return [*args, image_id, "-I", "-B", "-c", bootstrap, payload_sha256]


def owns_container(value, identity, name, owner, image_id):
    require(isinstance(identity, str) and CONTAINER_ID.fullmatch(identity), "ISOLATED_CONTAINER_ID")
    require(value.get("Id") == identity and value.get("Name") == "/" + name
            and value.get("Image") == image_id, "ISOLATED_CONTAINER_OWNERSHIP")
    labels = value.get("Config", {}).get("Labels", {})
    require(isinstance(labels, dict) and labels.get(OWNER_LABEL) == owner
            and labels.get(CONTRACT_LABEL) == CONTRACT_VERSION, "ISOLATED_CONTAINER_OWNERSHIP")


def container_contract(value, identity, name, owner, image_id, command, *, running=False, runtime_name):
    selected_runtime(runtime_name)
    owns_container(value, identity, name, owner, image_id)
    config, host, state = (value.get(key, {}) for key in ("Config", "HostConfig", "State"))
    require(config.get("User") == f"{UID}:{GID}" and config.get("WorkingDir") == "/work"
            and config.get("Entrypoint") == ["/usr/bin/python3"] and config.get("Cmd") == command
            and config.get("Hostname") == "mike-release-runner"
            and config.get("Image") == image_id, "ISOLATED_CONTAINER_EXECUTION_CONFIG")
    require(sorted(config.get("Env") or []) == sorted(key + "=" + val for key, val in ENVIRONMENT.items())
            and not config.get("Volumes") and not config.get("ExposedPorts")
            and config.get("Healthcheck", {}).get("Test") == ["NONE"]
            and config.get("Tty") is False and config.get("OpenStdin") is False,
            "ISOLATED_CONTAINER_ENVIRONMENT")
    seccomp_digest = security_options_contract(host.get("SecurityOpt"))
    require(host.get("NetworkMode") == "none" and host.get("ReadonlyRootfs") is True
            and host.get("Privileged") is False and host.get("CapDrop") == ["ALL"]
            and not host.get("CapAdd"),
            "ISOLATED_CONTAINER_SECURITY")
    require(host.get("PidMode", "") == "" and host.get("IpcMode") == "private"
            and host.get("UTSMode", "") == "" and host.get("UsernsMode", "") in ("", "private")
            and host.get("CgroupnsMode") == "private" and host.get("Runtime") == runtime_name,
            "ISOLATED_CONTAINER_NAMESPACES")
    require(host.get("Memory") == MEMORY_BYTES and host.get("MemorySwap") == MEMORY_BYTES
            and host.get("NanoCpus") == 2_000_000_000 and host.get("PidsLimit") == 256
            and host.get("ShmSize") == 64 * 1024 ** 2
            and host.get("Tmpfs") == {"/work": TMPFS}, "ISOLATED_CONTAINER_RESOURCE_LIMITS")
    for key in ("Binds", "Mounts", "VolumesFrom", "Devices", "DeviceRequests", "DeviceCgroupRules", "Links",
                "PortBindings", "ExtraHosts", "Dns", "DnsOptions", "DnsSearch", "GroupAdd", "Sysctls"):
        require(not host.get(key), "ISOLATED_CONTAINER_EXTERNAL_RESOURCE")
    require(host.get("PublishAllPorts") is False and host.get("AutoRemove") is False
            and host.get("RestartPolicy", {}).get("Name") == "no"
            and host.get("LogConfig", {}).get("Type") == "none", "ISOLATED_CONTAINER_LIFECYCLE")
    mounts = value.get("Mounts") or []
    require(isinstance(mounts, list) and all(item.get("Type") == "tmpfs"
            and item.get("Destination") == "/work" and not item.get("Source") for item in mounts),
            "ISOLATED_CONTAINER_MOUNTS")
    networks = value.get("NetworkSettings", {}).get("Networks", {})
    require(isinstance(networks, dict) and set(networks) <= {"none"}, "ISOLATED_CONTAINER_NETWORKS")
    require(state.get("Running") is running and not state.get("Paused") and not state.get("Restarting")
            and (state.get("Status") == "running" if running else state.get("Status") == "created"),
            "ISOLATED_CONTAINER_STATE")
    return {"configuration_sha256": hashlib.sha256(json.dumps({"config": config, "host": host},
            sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
            "network_mode": "none", "nonroot_uid": UID, "read_only_rootfs": True,
            "capabilities_dropped": "ALL", "no_new_privileges": True,
            "host_mounts": False, "daemon_observed": True, "seccomp_profile_sha256": seccomp_digest,
            "socket_domains_allowlisted": ["AF_UNIX", "AF_INET", "AF_INET6"],
            "indirect_socket_syscalls_denied": True, "native_amd64_syscalls_only": True}
