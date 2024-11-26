SHELL := bash
MAX_DRIVER_ID := 2

drivers: 
	for number in {0..${MAX_DRIVER_ID}} ; do \
        cd driver; (xterm -e cargo run $$number &) ; cd .. ; \
    done

.PHONY: build