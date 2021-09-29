# holo-auto-installer

## Usage

```
$ holo-auto-installer --help
USAGE:
    hpos-holo-auto-installer [OPTIONS] <happ-list-path>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
        --admin-port <admin-port>              Holochain conductor port [env: ADMIN_PORT=]  [default: 4444]
        --happ-port <happ-port>                hApp listening port [env: HAPP_PORT=]  [default: 42233]

ARGS:
    <happ-list-path>    Path to a YAML file containing the list of hApps to install
```

where file at `happ-list-path` is of a format:

```yaml
core_happs: [Happ]
self_hosted_happs: [Happ]
```

where `Happ` is

```yaml
app_id: string
version: string
dna_url: string (optional)
ui_url: string (optional)
```

Example YAML:

```yaml
---
core_happs:
  - app_id: hha
    version: 1
    dna_url: https://s3.eu-central-1.wasabisys.com/elemetal-chat-tests/hha.happ
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).
