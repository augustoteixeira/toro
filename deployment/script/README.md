# Deployment

The server runs on a VPS behind nginx with HTTPS, managed by supervisord.

## First installation

On the server:

```bash
cd ~
git clone https://github.com/yourusername/toro.git
sudo mv toro /opt
```

Compile either locally or on the server and place the binary at
`/opt/toro/server/target/release/server`.

Run once manually to verify migration and generate the API token:

```bash
cd /opt/toro/server
./target/release/server
```

Note the printed API token — it is shown only once.

## Supervisor

Install supervisor:

```bash
sudo apt update && sudo apt install supervisor
```

Create `/etc/supervisor/conf.d/toro.conf`:

```ini
[program:toro]
directory=/opt/toro/server
command=/opt/toro/server/target/release/server
autostart=true
autorestart=true
stderr_logfile=/var/log/toro.err.log
stdout_logfile=/var/log/toro.out.log
```

Then:

```bash
sudo supervisorctl update
```

## nginx

Add a server block to your nginx config (e.g. `/etc/nginx/sites-available/toro`):

```nginx
server {
    listen 443 ssl;
    server_name your.domain.com;

    ssl_certificate     /etc/letsencrypt/live/your.domain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/your.domain.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8008;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

Enable and reload:

```bash
sudo ln -s /etc/nginx/sites-available/toro /etc/nginx/sites-enabled/
sudo nginx -s reload
```

## Upgrading

Allow passwordless supervisorctl restart by adding to sudoers (`sudo visudo`):

```
youruser ALL=(ALL) NOPASSWD: /usr/bin/supervisorctl stop toro
youruser ALL=(ALL) NOPASSWD: /usr/bin/supervisorctl start toro
youruser ALL=(ALL) NOPASSWD: /usr/bin/supervisorctl status toro
```

Copy the deploy script template and fill in your credentials:

```bash
cp update_default update
# edit update: set VPS_USER and VPS_HOST
```

Then deploy with:

```bash
./update
```
