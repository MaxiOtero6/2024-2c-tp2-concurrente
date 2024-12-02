#!/bin/bash

# Text Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
CYAN='\033[0;36m'
WHITE='\033[0;37m'

# Aux PIDs
PAYMENT_BACKGROUND_PID=''
DRIVER_BACKGROUND_PID=''

assert_eq() {
  local expected="$1"
  local actual="$2"
  local msg="${3-}"

  if [ "$expected" == "$actual" ]; then
    echo -e "${GREEN}OK${NC}"
    return 0
  else
    [ "${#msg}" -gt 0 ] && log_failure "$expected == $actual :: $msg" || true
    echo -e "${RED}FAIL${NC}"
    echo -e "${RED}$msg${NC}"
    echo ""
    return 1
  fi
}

build_all() {
  cd driver || exit
  cargo build
  cd ..
  cd passenger || exit
  cargo build
  cd ..
  cd payment || exit
  cargo build
  cd ..
}

boot_driver() {
  local id="$1"
  local accept_trip_probability="$2"
  cd driver || exit
  TEST=true TAKE_TRIP_PROBABILITY="$accept_trip_probability" cargo run "$id"
  DRIVER_BACKGROUND_PID=$!
  echo DRIVER_BACKGROUND_PID
  sleep 100
  cd ..
}

boot_passenger() {
  local id="$1"
  local origin="$2"
  local destination="$3"
  cd passenger || exit
  cargo run id="$id" origin="$origin" dest="$destination"
  cd ..
}

boot_payment() {
  local accept_payment_probability="$1"
  cd payment || exit
  ACCEPT_CARD_PROBABILITY="$accept_payment_probability" cargo run &
  PAYMENT_BACKGROUND_PID=$!
  echo PAYMENT_BACKGROUND_PID
  sleep 100
  cd ..
}

kill_all() {
  kill -9 $PAYMENT_BACKGROUND_PID
  kill -9 $DRIVER_BACKGROUND_PID 
}

# Build all the projects
echo -e "${CYAN}Building all dependencies${WHITE}"
build_all

# Test 1 One Driver and One Passenger
echo -e "${CYAN}Test 1: One Driver and One Passenger${WHITE}"
boot_payment 1 &
boot_driver 0 1 &
boot_passenger 1 "(0,0)" "(10,10)"
passenger_exit_code=$?
assert_eq 0 passenger_exit_code "The exit code of the passenger was not the expected one."
kill_all