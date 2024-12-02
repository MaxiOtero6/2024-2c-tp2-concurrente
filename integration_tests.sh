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
DRIVER_1_BACKGROUND_PID=''
DRIVER_2_BACKGROUND_PID=''
PASSENGER_1_BACkGROUND_PID=''
PASSENGER_2_BACkGROUND_PID=''

log_failure() {
  local message="$1"
  echo "$message" >> error_log.txt
}

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

boot_driver_1() {
  local id="$1"
  local accept_trip_probability="$2"
  cd driver || exit
  TEST=true TAKE_TRIP_PROBABILITY="$accept_trip_probability" cargo run "$id" &> /dev/null &
  DRIVER_1_BACKGROUND_PID=$!
  sleep 1
  cd ..
}

boot_driver_2() {
  local id="$1"
  local accept_trip_probability="$2"
  cd driver || exit
  TEST=true TAKE_TRIP_PROBABILITY="$accept_trip_probability" cargo run "$id" &> /dev/null &
  DRIVER_2_BACKGROUND_PID=$!
  sleep 1
  cd ..
}

boot_passenger() {
  local id="$1"
  local origin="$2"
  local destination="$3"
  cargo run --manifest-path passenger/Cargo.toml id="$id" origin="$origin" dest="$destination" &> /dev/null
}

boot_passenger_1_in_background() {
  local id="$1"
  local origin="$2"
  local destination="$3"
  cargo run --manifest-path passenger/Cargo.toml id="$id" origin="$origin" dest="$destination" &> /dev/null &
  PASSENGER_1_BACkGROUND_PID=$!
}

boot_passenger_2_in_background() {
  local id="$1"
  local origin="$2"
  local destination="$3"
  cargo run --manifest-path passenger/Cargo.toml id="$id" origin="$origin" dest="$destination" &> /dev/null &
  PASSENGER_2_BACkGROUND_PID=$!
}

boot_payment() {
  local accept_payment_probability="$1"
  cd payment || exit
  ACCEPT_CARD_PROBABILITY="$accept_payment_probability" cargo run &> /dev/null &
  PAYMENT_BACKGROUND_PID=$!
  sleep 1
  cd ..
}

kill_all() {
  kill -9 "$PAYMENT_BACKGROUND_PID"
  [ -n "$DRIVER_1_BACKGROUND_PID" ] && kill -9 "$DRIVER_1_BACKGROUND_PID"
  [ -n "$DRIVER_2_BACKGROUND_PID" ] && kill -9 "$DRIVER_2_BACKGROUND_PID"
  echo -e "${MAGENTA}Killed all processes${WHITE}"
}

kill_all_passengers() {
  [ -n "$PASSENGER_1_BACkGROUND_PID" ] && kill -9 "$PASSENGER_1_BACkGROUND_PID"
  [ -n "$PASSENGER_2_BACkGROUND_PID" ] && kill -9 "$PASSENGER_2_BACkGROUND_PID"
  echo -e "${MAGENTA}Killed all passengers${WHITE}"
}

# Build all the projects
echo -e "${CYAN}Building all dependencies${WHITE}"
build_all

# Test 1: One Driver and One Passenger (Accepts)
echo -e "${CYAN}Test 1: One Available Driver and One Passenger${WHITE}"
boot_payment 1
boot_driver_1 0 1.0
boot_passenger 1 "(0,0)" "(10,10)"
assert_eq 0 $? "The exit code of the passenger was not the expected one."
kill_all

# Test 2: One Driver and One Passenger (Rejects)
echo -e "${CYAN}Test 2: One Non-Available Driver and One Passenger${WHITE}"
boot_payment 1
boot_driver_1 0 0.0
boot_passenger 1 "(0,0)" "(10,10)"
assert_eq 1 $? "The exit code of the passenger was not the expected one."
kill_all

# Test 3: One driver very far away from the passenger cannot accept the trip
echo -e "${CYAN}Test 3: One Driver very far away from the Passenger${WHITE}"
boot_payment 1
boot_driver_1 10 1.0
boot_passenger 1 "(0,0)" "(10,10)"
assert_eq 1 $? "The exit code of the passenger was not the expected one."
kill_all

# Test 4: One driver and two passengers (One will be rejected)
echo -e "${CYAN}Test 4: One Driver and Two Passengers${WHITE}"
boot_payment 1
boot_driver_1 0 1.0
boot_passenger_1_in_background 1 "(0,0)" "(10,10)"
boot_passenger_2_in_background 2 "(0,0)" "(15,15)"
wait "$PASSENGER_1_BACkGROUND_PID"
assert_eq 0 $? "The exit code of the first passenger was not the expected one."
wait "$PASSENGER_2_BACkGROUND_PID"
assert_eq 1 $? "The exit code of the second passenger was not the expected one."
kill_all


# Test 5: Two drivers and two passengers (Both will be accepted)
echo -e "${CYAN}Test 5: Two Drivers and Two Passengers${WHITE}"
boot_payment 1
boot_driver_1 0 1.0
boot_driver_2 1 1.0
boot_passenger_1_in_background 1 "(0,0)" "(10,10)"
boot_passenger_2_in_background 2 "(3,3)" "(13,13)"
wait "$PASSENGER_1_BACkGROUND_PID"
assert_eq 0 $? "The exit code of the first passenger was not the expected one."
wait "$PASSENGER_2_BACkGROUND_PID"
assert_eq 0 $? "The exit code of the second passenger was not the expected one."
kill_all
