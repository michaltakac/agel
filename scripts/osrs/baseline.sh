#!/bin/sh
set -eu
mkdir -p target/osrs-admin
agel_javac=javac
if test -n "${JAVA_HOME:-}" && test -x "$JAVA_HOME/bin/javac"; then
  agel_javac="$JAVA_HOME/bin/javac"
elif test -x /opt/homebrew/opt/openjdk@17/bin/javac; then
  agel_javac=/opt/homebrew/opt/openjdk@17/bin/javac
fi
"$agel_javac" -d target/osrs-admin scripts/osrs/Baseline.java
docker cp target/osrs-admin/Baseline.class agel-osrs-world:/tmp/Baseline.class
docker exec agel-osrs-world /bin/sh -c 'exec "$JAVA_HOME/bin/java" -cp "/tmp:/world/app/lib/*" Baseline'
