SHELL := bash
MAX_DRIVER_ID := 5

drivers: 
	for number in {0..${MAX_DRIVER_ID}} ; do \
        cd driver; (xterm -e "cargo run $$number 2>&1 | tee log$$number.log" &); sleep 0.1 ; cd .. ; \
    done

.PHONY: drivers