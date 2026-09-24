#!/bin/sh
# Start sshd, then hand over to the image's own entrypoint, which installs Joomla from the
# JOOMLA_* variables on first start and runs Apache.
set -e
/usr/sbin/sshd
# The install writes as www-data; the editor account reads through the group.
( while [ ! -f /var/www/html/configuration.php ]; do sleep 1; done
  chmod -R g+rX /var/www/html ) &
exec /entrypoint.sh "$@"
