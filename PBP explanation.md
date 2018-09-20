So the host should be broken up into main, IO manager, and TCP handlers (which are separate threads). main should have a task list 
like this:
```
- check for incoming connection
    - if YES, create TCP handler for incoming connection
    - if NO, carry on
- check for messages on IO manager receiver
    - if PROGRESS_QUERY (0), send PROGRESS_QUERY (0) to all TCP handlers
    - if PAUSE (1), send PAUSE (1) to all TCP handlers
    - if PLAY (2), send PLAY (2) to all TCP handlers
    - if SOFT_TERM (3), send SOFT_TERM (3) to all TCP handlers (finish all testing for current x value,
          then quit)
    - if HARD_TERM (4), send HARD_TERM (4) to all TCP handlers (quit immediately)
    - if NUM_CONNECTIONS (5), print number of TCP handlers
    - ignore anything else
- check to see if any receivers for TCP handlers have been dropped
- repeat all of the above until all TCP handlers have completed execution, or until qs is entered and current
      x values have been tested, or until qf is entered
```

main's IO manager should have something like this (spawned with 'spawn_io_manager()'):
```
- read stdin
    - if input is "qs", send SOFT_TERM (3) to main thread
    - if input is "qp", send PROGRESS_QUERY (0) to main thread
    - if input is "pause", send PAUSE (1) to main thread
    - if input is "play", send PLAY (2) to main thread
    - if input is "nc", send NUM_CONNECTIONS (5) to main thread
    - if input is "qf", send HARD_TERM (4) to main thread
    - if input is "h", print help dialog
- repeat until main finishes execution
```

TCP handlers should have a task list like this (spawned with 'spawn_handler_thread()'):
```
- check to see if it's time to send a heartbeat message
    - if YES, send heartbeat
    - if NO, carry on
- check to see if there's a byte on the TCP stream
    - if YES:
        - read the byte off the stream
        - match the byte (casted to a char)
            - if PROGRESS ('a'), the client sent a progress report. Print this out
            - if REQUEST ('r'), the client sent a data request. Reply to this by:
                - get a lock on the iterator
                - match the Option<u64>:
                    - if Some(u64), transmute to bytes and send to client
                    - if None, send TERMINATE ('t') then FINISHED ('f') to the client
            - if SOLUTION ('s'), the client sent a solution. Check this, and if it's valid, print it
            - if TERMINATING ('t'), the client is terminating. Close the thread
    - if NO, carry on
- check to see if there's an instruction from main on the instruction receiver
    - if YES:
        - match the instruction
            - if PROGRESS_QUERY (0), the instruction is progress query. Write 'c' then 0 to the client
            - if PAUSE (1), the instruction is pause. Write 'c' then 1 to the client
            - if PLAY (2), the instruction is play. Write 'c' then 2 to the client
            - if SOFT_TERMINATE (3), the instruction is soft terminate. Write 't' then 's' to the client
            - if HARD_TERMINATE (4), the instruction is hard terminate. Write 't' then 'h' to the client
    - if NO, carry on
- repeat until thread closes
- when thread closes, send a message to main over wrap_up_sender
```

Then the client also has a main and TCP handlers, but no IO manager. main should have a task list like this:
```
- spawn a constant number of tester threads
- check to see if there's anything to receive on wrap_up_receivers for all threads
    - if a thread has send a message on this channel, mark the thread as inactive
- when all threads are inactive, close main
```

Each TCP handler's task list should look like this:
```
- establish connection to host
- main loop:
    - check to see if thread should soft terminate (handled by soft_terminate boolean)
        - if YES, break loop
        - if NO, carry on
    - get X value
        - write REQUEST ('r') to host to request data
        - read next byte off of TCP stream
        - match byte (casted to a char)
            - if INCOMING_X ('x'), the next 8 bytes are the x value. Transmute them to a u64, and continue to
                  testing
            - if HEARTBEAT ('h'), respond with HEARTBEAT ('h'). This is the heartbeat. Repeat getting the X
                  value
            - if TERMINATE ('t'), break main loop
            - if COMMAND ('c'), match next byte
                - if PROGRESS_QUERY (0), the client has received a progress query. Since the client has no X,
                      Y, or Z value, send ['e' as u64, 'r' as u64, 'r' as u64] back
                - if PAUSE (1), the client has received a pause command. Pause loop:
                    - match next byte on the stream
                        - if HEARTBEAT ('h'), send HEARTBEAT ('h') back (heartbeat)
                        - if TERMINATE ('t'), break main loop
                        - if INCOMING_X ('x'), store new x value until done and return when pause loop is
                              broken
                        - if COMMAND ('c'), match next byte
                            - if PROGRESS_QUERY (0), send ['e' as u64, 'r' as u64, 'r' as u64] back
                            - if PAUSE (1), ignore
                            - if PLAY (2), break pause loop
                        - if TERMINATE ('t'), break main loop
                - if PLAY (2), ignore
                - repeat getting X value
    - for y in (2..x / 24).map(|y| y * 24)
        - for z in (2..(x - y) / 24).map(|z| z * 24).take_while(|&z| z < y)
            - test (x, y, z)
            - check TCP stream for byte
                - if a byte is available, match byte (casted to char)
                    - if COMMAND ('c'), match next byte from stream
                        - if PROGRESS_QUERY (0), send current [x, y, z]
                        - if PAUSE (1), pause. Pause loop:
                            - match next incoming byte on TCP stream (casted to char)
                                - if COMMAND ('c'), match next byte
                                    - if PROGRESS_QUERY (0), send current [x, y, z]
                                    - if PAUSE (1), ignore
                                    - if PLAY (2), break pause loop
                                - if TERMINATE ('t'), match next byte (casted to char)
                                    - if SOFT ('s'), set soft_term to true
                                    - if HARD ('h'), break main loop
                                    - ignore anything else
                                - if HEARTBEAT ('h'), send HEARTBEAT ('h') back (heartbeat)
                        - ignore anything else
                    - if TERMINATE, match next byte (casted to char)
                        - if SOFT ('s'), set soft_term to true
                        - if HARD ('h'), break main loop
                    - if HEARTBEAT ('h'), send HEARTBEAT ('h') back
                    - ignore anything else
                - if no byte is available, continue
- once main is broken, just close the thread
```