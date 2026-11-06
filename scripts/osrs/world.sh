#!/bin/sh
set -eu
case "${1:-status}" in
  start)
    docker run -d --platform linux/arm64 --name agel-osrs-world \
      --publish 127.0.0.1:43594:43594 \
      --mount source=agel-osrs-postgres,target=/world/.data/postgres \
      agel-osrs:241
    ;;
  stop) docker stop --timeout 45 agel-osrs-world ;;
  resume) docker start agel-osrs-world ;;
  replace)
    docker stop --timeout 45 agel-osrs-world
    docker rm agel-osrs-world
    exec sh "$0" start
    ;;
  logs) docker logs --tail 50 agel-osrs-world ;;
  status) docker inspect --format '{{.State.Status}} {{json .NetworkSettings.Ports}}' agel-osrs-world ;;
  *) echo 'usage: world.sh start|stop|resume|replace|logs|status' >&2; exit 2 ;;
esac
