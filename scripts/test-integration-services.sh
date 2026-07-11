#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

run_id="nidus-it-$$-${RANDOM}"
label_key="io.nidus.integration.run"
network="${run_id}"
temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/nidus-integration.XXXXXX")"
git_before="$(git status --porcelain=v1 --untracked-files=all)"
volumes_before="$(docker volume ls -q | LC_ALL=C sort)"
ports=()
redis_port=""
mysql_port=""
cockroach_port=""
kafka_port=""
nats_port=""
rabbit_port=""
sqs_port=""

cleanup_resources() {
  set +e
  while IFS= read -r container_id; do
    [[ -n "${container_id}" ]] && docker rm -fv "${container_id}" >/dev/null 2>&1
  done <<<"$(docker ps -aq --filter "label=${label_key}=${run_id}")"
  while IFS= read -r network_id; do
    [[ -n "${network_id}" ]] && docker network rm "${network_id}" >/dev/null 2>&1
  done <<<"$(docker network ls -q --filter "label=${label_key}=${run_id}")"
  while IFS= read -r volume_id; do
    [[ -n "${volume_id}" ]] && docker volume rm -f "${volume_id}" >/dev/null 2>&1
  done <<<"$(docker volume ls -q --filter "label=${label_key}=${run_id}")"
  rm -rf "${temp_dir}"
  set -e
}

verify_cleanup() {
  if [[ -n "$(docker ps -aq --filter "label=${label_key}=${run_id}")" ]]; then
    echo "integration containers remain after cleanup" >&2
    return 1
  fi
  if [[ -n "$(docker network ls -q --filter "label=${label_key}=${run_id}")" ]]; then
    echo "integration networks remain after cleanup" >&2
    return 1
  fi
  if [[ -n "$(docker volume ls -q --filter "label=${label_key}=${run_id}")" ]]; then
    echo "integration volumes remain after cleanup" >&2
    return 1
  fi
  local volumes_after
  volumes_after="$(docker volume ls -q | LC_ALL=C sort)"
  if [[ "${volumes_before}" != "${volumes_after}" ]]; then
    echo "integration suite changed the Docker volume inventory" >&2
    diff -u <(printf '%s\n' "${volumes_before}") <(printf '%s\n' "${volumes_after}") >&2 || true
    return 1
  fi
  if [[ -e "${temp_dir}" ]]; then
    echo "integration temporary directory remains after cleanup" >&2
    return 1
  fi
  ruby -rsocket -e 'sockets = ARGV.map { |port| TCPServer.new("127.0.0.1", Integer(port)) }; sockets.each(&:close)' "${ports[@]}"
  local git_after
  git_after="$(git status --porcelain=v1 --untracked-files=all)"
  if [[ "${git_before}" != "${git_after}" ]]; then
    echo "integration suite changed the worktree" >&2
    diff -u <(printf '%s\n' "${git_before}") <(printf '%s\n' "${git_after}") >&2 || true
    return 1
  fi
}

on_exit() {
  local status=$?
  trap - EXIT INT TERM HUP
  cleanup_resources
  if verify_cleanup; then
    echo "[integration] cleanup proof after status ${status}: 0 containers, 0 networks, 0 volumes, 0 temp paths, all ports released, worktree unchanged"
  else
    status=1
  fi
  exit "${status}"
}
trap on_exit EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

free_port() {
  ruby -rsocket -e 'server = TCPServer.new("127.0.0.1", 0); puts server.addr[1]; server.close'
}

allocate_port() {
  local variable_name="$1"
  local port
  port="$(free_port)"
  ports+=("${port}")
  printf -v "${variable_name}" '%s' "${port}"
}

wait_for() {
  local description="$1"
  shift
  for _ in $(seq 1 90); do
    if "$@" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  echo "timed out waiting for ${description}" >&2
  docker ps -a --filter "label=${label_key}=${run_id}" >&2
  while IFS= read -r failed_container; do
    if [[ -n "${failed_container}" ]]; then
      docker logs "${failed_container}" >&2 || true
    fi
  done <<<"$(docker ps -aq --filter "label=${label_key}=${run_id}")"
  return 1
}

remove_container() {
  docker rm -fv "$1" >/dev/null
}

docker network create --label "${label_key}=${run_id}" "${network}" >/dev/null

echo "[integration] Redis 8"
allocate_port redis_port
redis_name="${run_id}-redis"
docker run -d --name "${redis_name}" \
  --label "${label_key}=${run_id}" \
  --network "${network}" \
  -p "127.0.0.1:${redis_port}:6379" \
  redis:8-alpine redis-server --save '' --appendonly no >/dev/null
wait_for Redis docker exec "${redis_name}" redis-cli ping
if [[ "${NIDUS_INTEGRATION_FAILPOINT:-}" == "redis-ready" ]]; then
  echo "[integration] injecting requested failure after Redis readiness" >&2
  exit 97
fi
redis_test_panic=0
if [[ "${NIDUS_INTEGRATION_FAILPOINT:-}" == "redis-test-panic" ]]; then
  redis_test_panic=1
fi
NIDUS_TEST_REDIS_URL="redis://127.0.0.1:${redis_port}/0" \
  NIDUS_TEST_INJECT_PANIC="${redis_test_panic}" \
  cargo test -p nidus-redis --all-features --test live -- --ignored --exact real_redis_round_trip_ttl_health_and_cleanup
remove_container "${redis_name}"

echo "[integration] MySQL 8.4 and durable jobs"
allocate_port mysql_port
mysql_name="${run_id}-mysql"
mysql_password="nidustest${RANDOM}${RANDOM}"
docker run -d --name "${mysql_name}" \
  --label "${label_key}=${run_id}" \
  --network "${network}" \
  -p "127.0.0.1:${mysql_port}:3306" \
  -e "MYSQL_ROOT_PASSWORD=${mysql_password}" \
  -e MYSQL_DATABASE=nidus \
  mysql:8.4 --skip-log-bin >/dev/null
wait_for MySQL docker exec "${mysql_name}" mysql -uroot "-p${mysql_password}" \
  --database=nidus --execute 'SELECT 1'
mysql_url="mysql://root:${mysql_password}@127.0.0.1:${mysql_port}/nidus?ssl-mode=DISABLED"
NIDUS_TEST_MYSQL_URL="${mysql_url}" \
  cargo test -p nidus-sqlx --all-features --test live_services -- --ignored --exact real_mysql_pool_round_trip_and_cleanup
NIDUS_TEST_JOBS_MYSQL_URL="${mysql_url}" \
  cargo test -p nidus-jobs-sqlx --all-features --test live_services -- --ignored --exact real_mysql_store_is_multi_worker_safe
remove_container "${mysql_name}"

echo "[integration] CockroachDB 26.2 verify-full TLS and retries"
cockroach_image="cockroachdb/cockroach:v26.2.0"
allocate_port cockroach_port
cockroach_name="${run_id}-cockroach"
cert_dir="${temp_dir}/cockroach-certs"
mkdir -p "${cert_dir}"
chmod 700 "${cert_dir}"
for cert_command in ca node client; do
  generator_name="${run_id}-cert-${cert_command}"
  case "${cert_command}" in
    ca)
      docker run --rm --name "${generator_name}" \
        --label "${label_key}=${run_id}" \
        --user "$(id -u):$(id -g)" \
        -v "${cert_dir}:/certs" \
        "${cockroach_image}" cert create-ca --certs-dir=/certs --ca-key=/certs/ca.key
      ;;
    node)
      docker run --rm --name "${generator_name}" \
        --label "${label_key}=${run_id}" \
        --user "$(id -u):$(id -g)" \
        -v "${cert_dir}:/certs" \
        "${cockroach_image}" cert create-node localhost 127.0.0.1 ::1 \
        --certs-dir=/certs --ca-key=/certs/ca.key
      ;;
    client)
      docker run --rm --name "${generator_name}" \
        --label "${label_key}=${run_id}" \
        --user "$(id -u):$(id -g)" \
        -v "${cert_dir}:/certs" \
        "${cockroach_image}" cert create-client root \
        --certs-dir=/certs --ca-key=/certs/ca.key
      ;;
  esac
done
chmod 644 "${cert_dir}"/*.crt
chmod 600 "${cert_dir}"/*.key
docker run -d --name "${cockroach_name}" \
  --label "${label_key}=${run_id}" \
  --network "${network}" \
  -p "127.0.0.1:${cockroach_port}:26258" \
  -v "${cert_dir}:/certs:ro" \
  "${cockroach_image}" start-single-node \
  --certs-dir=/certs \
  --listen-addr=localhost:26257 \
  --sql-addr=0.0.0.0:26258 \
  --advertise-sql-addr=localhost:26258 \
  --http-addr=0.0.0.0:8080 >/dev/null
wait_for CockroachDB docker exec "${cockroach_name}" cockroach sql \
  --certs-dir=/certs --host=localhost:26258 --database=defaultdb --execute 'SELECT 1'
cockroach_url="postgresql://root@localhost:${cockroach_port}/defaultdb?sslmode=verify-full&sslrootcert=${cert_dir}/ca.crt&sslcert=${cert_dir}/client.root.crt&sslkey=${cert_dir}/client.root.key"
NIDUS_TEST_COCKROACH_URL="${cockroach_url}" \
  cargo test -p nidus-sqlx --all-features --test live_services -- --ignored --exact real_cockroach_verify_full_tls_and_injected_serialization_retries
NIDUS_TEST_JOBS_COCKROACH_URL="${cockroach_url}" \
  cargo test -p nidus-jobs-sqlx --all-features --test live_services -- --ignored --exact real_cockroach_tls_store_is_multi_worker_safe
remove_container "${cockroach_name}"

echo "[integration] Apache Kafka 4.0"
allocate_port kafka_port
kafka_name="${run_id}-kafka"
docker run -d --name "${kafka_name}" \
  --label "${label_key}=${run_id}" \
  --network "${network}" \
  -p "127.0.0.1:${kafka_port}:9092" \
  -e KAFKA_NODE_ID=1 \
  -e KAFKA_PROCESS_ROLES=broker,controller \
  -e KAFKA_LISTENERS=PLAINTEXT://:9092,CONTROLLER://:9093 \
  -e "KAFKA_ADVERTISED_LISTENERS=PLAINTEXT://127.0.0.1:${kafka_port}" \
  -e KAFKA_CONTROLLER_LISTENER_NAMES=CONTROLLER \
  -e KAFKA_LISTENER_SECURITY_PROTOCOL_MAP=CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT \
  -e KAFKA_CONTROLLER_QUORUM_VOTERS=1@localhost:9093 \
  -e KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR=1 \
  -e KAFKA_TRANSACTION_STATE_LOG_REPLICATION_FACTOR=1 \
  -e KAFKA_TRANSACTION_STATE_LOG_MIN_ISR=1 \
  -e KAFKA_GROUP_INITIAL_REBALANCE_DELAY_MS=0 \
  apache/kafka:4.0.0 >/dev/null
wait_for Kafka ruby -rsocket -e "socket = TCPSocket.new('127.0.0.1', ${kafka_port}); socket.close"
sleep 2
NIDUS_TEST_KAFKA_BROKERS="127.0.0.1:${kafka_port}" \
  cargo test -p nidus-kafka --all-features --test live -- --ignored --exact real_kafka_admin_delivery_consume_commit_and_cleanup
remove_container "${kafka_name}"

echo "[integration] NATS 2.11 JetStream"
allocate_port nats_port
nats_name="${run_id}-nats"
docker run -d --name "${nats_name}" \
  --label "${label_key}=${run_id}" \
  --network "${network}" \
  -p "127.0.0.1:${nats_port}:4222" \
  nats:2.11-alpine -js >/dev/null
wait_for NATS ruby -rsocket -e "socket = TCPSocket.new('127.0.0.1', ${nats_port}); socket.close"
NIDUS_TEST_NATS_URL="nats://127.0.0.1:${nats_port}" \
  cargo test -p nidus-nats --all-features --test live -- --ignored --exact real_jetstream_persistence_durable_consumer_ack_and_cleanup
remove_container "${nats_name}"

echo "[integration] RabbitMQ 4.1"
allocate_port rabbit_port
rabbit_name="${run_id}-rabbitmq"
rabbit_dir="${temp_dir}/rabbitmq"
mkdir -p "${rabbit_dir}"
chmod 0777 "${rabbit_dir}"
docker run -d --name "${rabbit_name}" \
  --label "${label_key}=${run_id}" \
  --network "${network}" \
  -p "127.0.0.1:${rabbit_port}:5672" \
  -v "${rabbit_dir}:/var/lib/rabbitmq" \
  -e "RABBITMQ_ERLANG_COOKIE=nidustest${RANDOM}${RANDOM}" \
  rabbitmq:4.1-alpine >/dev/null
wait_for RabbitMQ docker exec "${rabbit_name}" rabbitmq-diagnostics -q ping
NIDUS_TEST_RABBITMQ_URL="amqp://guest:guest@127.0.0.1:${rabbit_port}/%2f" \
  cargo test -p nidus-rabbitmq --all-features --test live -- --ignored --exact real_rabbitmq_confirm_consume_ack_and_cleanup
remove_container "${rabbit_name}"

echo "[integration] AWS SQS via LocalStack 4.6"
allocate_port sqs_port
sqs_name="${run_id}-sqs"
docker run -d --name "${sqs_name}" \
  --label "${label_key}=${run_id}" \
  --network "${network}" \
  -p "127.0.0.1:${sqs_port}:4566" \
  -e SERVICES=sqs \
  -e EAGER_SERVICE_LOADING=1 \
  localstack/localstack:4.6.0 >/dev/null
wait_for SQS curl --fail --silent "http://127.0.0.1:${sqs_port}/_localstack/health"
NIDUS_TEST_SQS_ENDPOINT="http://127.0.0.1:${sqs_port}" \
  cargo test -p nidus-sqs --all-features --test live -- --ignored --exact real_sqs_emulator_dlq_send_receive_delete_and_cleanup
remove_container "${sqs_name}"
