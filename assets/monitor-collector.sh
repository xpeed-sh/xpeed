export LC_ALL=C
if ! test -r /proc/stat || ! test -r /proc/meminfo; then
  echo 'Monitor requires Linux procfs' >&2
  exit 1
fi
while :; do
  printf 'BEGIN\nCPU\n' || exit
  cat /proc/stat
  printf 'MEMORY\n'
  cat /proc/meminfo
  printf 'UPTIME\n'
  cat /proc/uptime
  printf 'NETWORK\n'
  cat /proc/net/dev
  printf 'DISK\n'
  df -Pk / 2>/dev/null
  printf 'PAGESIZE\n'
  getconf PAGESIZE 2>/dev/null
  printf 'PROCESSES\n'
  cat /proc/[0-9]*/stat 2>/dev/null
  printf 'END\n' || exit
  sleep 3
done
