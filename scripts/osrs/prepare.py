#!/usr/bin/env python3
"""Build the pinned private-world image; caches and keys stay under target/."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request
import argparse

parser = argparse.ArgumentParser()
parser.add_argument("--reuse-cache", action="store_true", help="Rebuild code against the already installed cache")
args = parser.parse_args()

ROOT = Path(__file__).resolve().parents[2]
SERVER = ROOT / "target/external/openrune"
STAGE = ROOT / "target/osrs-container"
COMMIT = "c4cd45ef2d38a0badcc7a9f89773f8f1c29bb16f"
JRE = "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.13%2B9/OpenJDK21U-jre_aarch64_linux_hotspot_21.0.13_9.tar.gz"
JRE_SHA = "b1734db7ade49c3f6acc6ae06233b8c2dd90e3e2f00f67000ae1c972aaa64d8c"
BASE = "python:3.12-slim-bookworm@sha256:e0b3f8d16a5ccfbe602d5c94297e5dbcda2ec256edca2b80fcd44e39e1ff6feb"
PG_URL = "https://repo.maven.apache.org/maven2/io/zonky/test/postgres/embedded-postgres-binaries-linux-arm64v8/14.22.0/embedded-postgres-binaries-linux-arm64v8-14.22.0.jar"
PG_SHA = "60dd44d00ff2aaf62c43137e4c8bdb89b0ca044f4eac285eefc39c051468781d"


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


if not SERVER.exists():
    SERVER.parent.mkdir(parents=True, exist_ok=True)
    run("git", "clone", "https://github.com/OpenRune/OpenRune-Server.git", str(SERVER))
    run("git", "checkout", "--detach", COMMIT, cwd=SERVER)
actual = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=SERVER, text=True).strip()
if actual != COMMIT:
    raise SystemExit("Server checkout differs from the pinned commit; use a separate checkout.")
patch = ROOT / "scripts/osrs/openrune.patch"
for candidate in [patch, ROOT / "scripts/osrs/admin.patch"]:
    if subprocess.run(["git", "apply", "--reverse", "--check", str(candidate)], cwd=SERVER,
                      capture_output=True).returncode:
        restored = None
        if candidate == patch:
            relative = "api/net/src/main/kotlin/org/rsmod/api/net/rsprot/NetworkFactory.kt"
            original = subprocess.check_output(["git", "show", "HEAD:" + relative], cwd=SERVER, text=True)
            legacy = original.replace("    override val ports:",
                '    override val host: String = System.getenv("AGEL_GAME_BIND") ?: "127.0.0.1"\n\n    override val ports:')
            file = SERVER / relative
            if file.read_text() == legacy:
                restored = (file, legacy)
                file.write_text(original)
        try:
            run("git", "apply", "--check", str(candidate), cwd=SERVER)
            run("git", "apply", str(candidate), cwd=SERVER)
        except Exception:
            if restored:
                restored[0].write_text(restored[1])
            raise
env = dict(os.environ)
mac_java = Path("/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home")
if mac_java.exists():
    env["JAVA_HOME"] = str(mac_java)
if args.reuse_cache and not (SERVER / ".data/cache/SERVER").is_dir():
    raise SystemExit("--reuse-cache requires an installed cache")
tasks = [":server:app:installDist"] if args.reuse_cache else ["install", ":server:app:installDist"]
run("./gradlew", *tasks, "--no-configuration-cache",
    "--console=plain", cwd=SERVER, env=env)
STAGE.mkdir(parents=True, exist_ok=True)
for name in ["app", ".data", "java"]:
    if (STAGE / name).exists():
        shutil.rmtree(STAGE / name)
shutil.copytree(SERVER / "server/app/build/install/app", STAGE / "app")
pg = ROOT / "target/external/postgres-linux-arm64.jar"
if not pg.exists():
    urllib.request.urlretrieve(PG_URL, pg)
if hashlib.sha256(pg.read_bytes()).hexdigest() != PG_SHA:
    raise SystemExit("Linux PostgreSQL checksum mismatch")
shutil.copyfile(pg, STAGE / "app/lib/postgres-linux-arm64.jar")
shutil.copytree(SERVER / ".data", STAGE / ".data",
                ignore=shutil.ignore_patterns("postgres", "postgres-*", "logs"))
(STAGE / ".data/postgres").mkdir()
config = (SERVER / "game.yml").read_text()
config = config.replace('mode: "assume-completed"', 'mode: "respect-progress"')
if 'mode: "respect-progress"' not in config:
    raise SystemExit("Quest-progress profile is missing; inspect game.yml.")
(STAGE / "game.yml").write_text(config)
archive = ROOT / "target/external/osrs-jre-linux.tar.gz"
if not archive.exists():
    urllib.request.urlretrieve(JRE, archive)
if hashlib.sha256(archive.read_bytes()).hexdigest() != JRE_SHA:
    raise SystemExit("Linux JRE checksum mismatch")
with tarfile.open(archive) as source:
    source.extractall(STAGE / "java", filter="data")
java_home = next((STAGE / "java").iterdir()).name
manifest = {"server": COMMIT, "revision": 241, "quest_mode": "respect-progress", "login_pow": "disabled-private-profile",
            "jre_sha256": JRE_SHA, "postgres_sha256": PG_SHA, "base": BASE,
            "patch_sha256": hashlib.sha256(patch.read_bytes()).hexdigest()}
manifest["admin_patch_sha256"] = hashlib.sha256((ROOT / "scripts/osrs/admin.patch").read_bytes()).hexdigest()
(STAGE / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
(STAGE / "Dockerfile").write_text(f"""FROM {BASE}
RUN groupadd --gid 1000 agel && useradd --uid 1000 --gid 1000 --create-home agel
COPY --chown=agel:agel . /world/
ENV JAVA_HOME=/world/java/{java_home} AGEL_GAME_BIND=0.0.0.0 AGEL_PRIVATE_NO_POW=1
WORKDIR /world
USER agel
VOLUME /world/.data/postgres
ENTRYPOINT ["/world/java/{java_home}/bin/java", "-Xms512m", "-Xmx4g", "-cp", "/world/app/lib/*", "org.rsmod.server.app.GameServerKt"]
""")
run("docker", "build", "--platform", "linux/arm64", "--pull=false", "-t", "agel-osrs:241", str(STAGE))
print("Built agel-osrs:241. Start with scripts/osrs/world.sh start.")
