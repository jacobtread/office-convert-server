# Office convert server 

![License](https://img.shields.io/github/license/jacobtread/office-convert-server?style=for-the-badge)
![Build](https://img.shields.io/github/actions/workflow/status/jacobtread/office-convert-server/build.yml?style=for-the-badge)

Simple server for converting office file formats into PDF files built on top of LibreOffice using https://github.com/jacobtread/libreofficekit

This repository contains two separate crates, the first being `office-convert-server` which is the binary crate for the server itself. The second is `office-convert-client` in the client directory which is a library crate providing a client for interacting with the server as well as providing a load balancing implementation.

## Running the server 

> [!IMPORTANT]
>
> It's important to take note of the LibreOffice version you are using, as per [LibreOffice Support](https://github.com/jacobtread/libreofficekit?tab=readme-ov-file#libreoffice-support) some newer versions of LibreOffice will have problems with segfaults after conversions so I recommend you have the server managed by an external process that will restart it (along with using the load balancer which will be able to wait for the server to be available)
>
> Alternatively use the older version listed above in order to avoid segfaults. This is a bug in LibreOffice itself and not in this server / backing library

### Precompiled binaries

Linux binaries are compiled against the version of glibc used by the Debian Bookworm Rust Docker image. If you version of glibc is different
you will likely be unable to run the binary. For this case you will need to build the binary yourself.

Below are links to pre-compiled binaries:

| Platform | Download                                                                                                           |
| -------- | ------------------------------------------------------------------------------------------------------------------ |
| Linux    | [Download](https://github.com/jacobtread/office-convert-server/releases/latest/download/office-convert-server)     |
| Windows  | [Download](https://github.com/jacobtread/office-convert-server/releases/latest/download/office-convert-server.exe) |


You can find individual releases on the [Releases](https://github.com/jacobtread/office-convert-server/releases) page


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

Update `LIBREOFFICE_SDK_PATH` your specific LibreofficeSDK path. This is **NOT** your Libreoffice path.

On Linux often times you will be able to omit this if your install is at the common paths of `/usr/lib64/libreoffice/program`, `/usr/lib/libreoffice/program`,
or `/opt/libreoffice-{VERSION}/program`.

## Building with docker

You can build the binary within docker using the following steps:

### Build docker image

Build the docker image from the Dockerfile

```
docker build -t office_convert .
```

### Create temp container

Create a temporary container for the converter

```
docker create --name temp_container office_convert
```

### Copy binary from temp container

Then copy the binary from the container to your host:

```
docker cp temp_container:/app/office-convert-server ./office-convert-server
```

### Cleanup the temp container

Once you have the binary on your host, you can remove the temporary container:

```
docker rm temp_container
```

## Available Endpoints

Below are the available endpoints, these are all accessible through the provided `office-convert-client` Rust client library.

### GET /status (Server status)

Obtains the current status of the server, used to check if the server is currently busy processing a document. 

#### Example Response

```json
{
	"is_busy": false
}
```

### GET /office-version (LibreOffice version details)

Reports version information for the underlying LibreOffice instance 

#### Example Response

```json
{
	"major": 24,
	"minor": 2,
	"build_id": "420(Build:2)"
}
```

> [!NOTE]
> 
> Will return 404 error if the LibreOffice version is too old to support this functionality

### GET /supported-formats (Formats supported by the server)

Reports the file mime types supported by the LibreOffice install

#### Example Response

```json
[
	{
		"name": "writer_MS_Word_95",
		"mime": "application/msword"
	},
    // ...remaining formats truncated for example
]
```

> [!NOTE]
> 
> Will return 404 error if the LibreOffice version is too old to support this functionality

### POST /convert (Convert a file)

Upload a file for conversion, this takes a multipart form data POST request containing 
a "file" field which is the file to convert.

Will respond with the file converted to PDF format as bytes

### POST /collect-garbage (Tell LibreOffice to clean up memory)

Takes in no arguments, will always respond with a 200 OK status. Office will be told to collect garbage after any other
waiting requests are processed

## Rust client library (office-convert-client)

### Usage without load balancer

Below is an example using the converter without the load balancer:

```rust
use office_convert_client::{OfficeConvertClient, ConvertOffice};

// Create a client
let convert_client = OfficeConvertClient::new("http://localhost:3000").unwrap();

let bytes = vec![/* Bytes to convert */]

// Convert the bytes
let converted = convert_client.convert(bytes).await.unwrap();
```

> [!NOTE]
>
> I recommend using the load balancer even if you've only got one client, as it will provide
> tolerance if the server fails / is unavailable 

Clients on their own provide functions for all the endpoints mentioned above

### Usage with load balancer

```rust
use office_convert_client::{OfficeConvertClient, ConvertOffice, OfficeConvertLoadBalancer};

// Create a client
let convert_client = OfficeConvertClient::new("http://localhost:3000").unwrap();

// Create a convert load balancer
let convert_load_balancer = OfficeConvertLoadBalancer::new(vec![convert_client]);

let bytes = vec![/* Bytes to convert */]

// Convert the bytes
let converted = convert_load_balancer.convert(bytes).await.unwrap();
```