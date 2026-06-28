#!/usr/bin/env bash

psql_smoke() {
  psql "$(postgres_url)" "$@"
}

sql_scalar() {
  psql_smoke -Atqc "$1"
}
