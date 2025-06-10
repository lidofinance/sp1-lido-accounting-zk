#!/bin/sh

# Strip leading and trailing quotes (both " and ') from INTERNAL_SCHEDULER_CRON

# THis is needed  to allow using the same .env file for both docker (which normally wants unquoted)
# and host machine runs via `source .env` and/or justfile commands
# Practically we just tell the container to accept quoted values and strip the quotes at launch
if [ -n "$INTERNAL_SCHEDULER_CRON" ]; then
  INTERNAL_SCHEDULER_CRON=$(echo "$INTERNAL_SCHEDULER_CRON" | sed -e 's/^["'\'']//; s/["'\'']$//')
  export INTERNAL_SCHEDULER_CRON
fi

if [ -n "$SERVICE_BIND_TO_ADDR" ]; then
  SERVICE_BIND_TO_ADDR=$(echo "$SERVICE_BIND_TO_ADDR" | sed -e 's/^["'\'']//; s/["'\'']$//')
  export SERVICE_BIND_TO_ADDR
fi



# For debugging
echo "CRON schedule after stripping quotes: [$INTERNAL_SCHEDULER_CRON]"
echo "CRON schedule after stripping quotes: [$SERVICE_BIND_TO_ADDR]"

# Continue with your app startup...
"$@" &
child=$!

# Forward signals to the child process
trap 'kill -TERM $child 2>/dev/null' INT TERM

# Wait for the child process to exit
wait $child