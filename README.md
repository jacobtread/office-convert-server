# Office convert server 

Simple server for converting office file formats into PDF files built on top of LibreOffice using https://github.com/jacobtread/libreofficekit

This repository contains two separate crates, the first being `office-convert-server` which is the binary crate for the server itself. The second is `office-convert-client` in the client directory which is a library crate providing a client for interacting with the server as well as providing a load balancing implementation.

## Running the server 

> [!IMPORTANT]
>
> It's important to take note of the LibreOffice version you are using, as per [LibreOffice Support](https://github.com/jacobtread/libreofficekit?tab=readme-ov-file#libreoffice-support) some newer versions of LibreOffice will have problems with segfaults after conversions so I recommend you have the server managed by an external process that will restart it (along with using the load balancer which will be able to wait for the server to be available)
>
> Alternatively use the older version listed above in order to avoid segfaults. This is a bug in LibreOffice itself and not in this server / backing library

### Server CLI arguments

You can provide arguments to the server to control its behavior:

| Argument               | Short Form | Required | Default                   | Description                                     |
| ---------------------- | ---------- | -------- | ------------------------- | ----------------------------------------------- |
| `--office-path <path>` | None       | No       | Attempt from common paths | Path to the office /program installation folder |
| `--host <host>`        | None       | No       | 0.0.0.0                   | Host to bind the server on                      |
| `--port <port>`        | None       | No       | 3000                      | Port to bind the server on                      |
| `--version`            | `-V`       | No       |                           | Logs the server version information             |
| `--help`               | `-h`       | No       |                           | Shows the available commands                    |

> [!NOTE]
>
> Command line arguments take priority over environment variables and other defaults

### Environment variables

| Variable Name          | Required | Default      | Description                                                                                                                                                                                               |
| ---------------------- | -------- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `LIBREOFFICE_SDK_PATH` | No       |              | Path to the office /program installation folder                                                                                                                                                           |
| `SERVER_ADDRESS`       | No       | 0.0.0.0:3000 | Specifies the socket address to bind the server to                                                                                                                                                        |
| `RUST_LOG`             | No       |              | Controls the logging behavior, see [Filtering Events with Environment Variables](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/fmt/index.html#filtering-events-with-environment-variables) |


## Requirements

Requires LibreOffice 

Debian:

```
sudo apt-get install libreoffice
```
> [!NOTE]
> On Debian bullseye you may not have the latest version of the libreoffice package. Which may result in some functionality being unavailable. you can use Debian Bookworm with the "Bookworm Backports" package repo, which you can add using the following command:
> ```sh
> echo "deb http://deb.debian.org/debian bookworm-backports main" | sudo tee /etc/apt/sources.list.d/bookworm-backports.list > /dev/null 
> ```
> The install libreoffice through that package repo:
> ```sh
> sudo apt-get -t bookworm-backports install -y libreoffice
> ```

Fedora:

```sh
sudo dnf install libreoffice
```

## Env variables
 
The server requires the following environment variables. 

> .env files will be loaded from the same and parent directories of the server

```
SERVER_ADDRESS=0.0.0.0:3000
LIBREOFFICE_SDK_PATH=/usr/lib64/libreoffice/program
```

Update `LIBREOFFICE_SDK_PATH` your specific LibreofficeSDK path. This is **NOT** your Libreoffice path

# Building with docker

You can build the binary within docker using the following steps:

## Build docker image

Build the docker image from the Dockerfile

```
docker build -t office_convert .
```

## Create temp container

Create a temporary container for the converter

```
docker create --name temp_container office_convert
```

## Copy binary from temp container

Then copy the binary from the container to your host:

```
docker cp temp_container:/app/office-convert-server ./office-convert-server
```

## Cleanup the temp container

Once you have the binary on your host, you can remove the temporary container:

```
docker rm temp_container
```