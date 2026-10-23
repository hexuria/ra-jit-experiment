#!/bin/bash
# Swap each demo step into hot.rs, waiting for the watcher's build cycle
# (desktop + ios-sim) to finish before the next — fastest the daemon allows.
cd /Volumes/goldcoders/stack-native/app
for i in 01 02 03 04 05 06 07 08 09 10 11 12 13; do
  before=$(grep -c "Finished running" /tmp/watcher.log)
  cp "/tmp/steps/$i.rs" src/hot.rs
  for t in $(seq 1 200); do
    now=$(grep -c "Finished running" /tmp/watcher.log)
    [ "$now" -gt "$before" ] && break
    sleep 0.1
  done
  echo "step $i landed ($(grep -c 'Finished running' /tmp/watcher.log) cycles)"
  sleep 0.4
done
echo ALL DONE
