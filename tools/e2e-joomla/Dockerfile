# Joomla 5 with an sshd beside Apache, so the e2e suite reaches the site the way production does:
# over SSH, as an ordinary account that can read the install (002 research R9, R10).
FROM docker.io/library/joomla:5-php8.2-apache

RUN apt-get update \
 && apt-get install -y --no-install-recommends openssh-server \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /run/sshd \
 && useradd --create-home --shell /bin/bash --groups www-data editor \
 && echo 'editor:e2e-editor-password' | chpasswd \
 && mkdir -p /srv/newsbuilder-photos && chown editor:editor /srv/newsbuilder-photos \
 && printf 'PasswordAuthentication yes\nPermitRootLogin no\n' > /etc/ssh/sshd_config.d/e2e.conf

COPY entrypoint.sh /usr/local/bin/e2e-entrypoint.sh
RUN chmod +x /usr/local/bin/e2e-entrypoint.sh
ENTRYPOINT ["/usr/local/bin/e2e-entrypoint.sh"]
CMD ["apache2-foreground"]
