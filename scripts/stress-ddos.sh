#!/usr/bin/env bash

# parallel -j 1 --progress "curl -s 'http://localhost:8080/tiles/osm/14/8192/{}' | wc -c" ::: {5700..10000}
time parallel -j 10 --progress "curl -s -H 'accept-encoding:br' 'http://localhost:8080/tiles/osm/14/8192/{}' > /dev/null" ::: {5700..6000}
