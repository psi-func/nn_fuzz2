docker run \
    -p 9090:9090 \
    -v $(pwd)/conf/prometheus.yml:/etc/prometheus/prometheus.yml \
    prom/prometheus