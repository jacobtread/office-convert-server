# Office convert server 

Simple server for converting office file formats into PDF files built on top of LibreOffice

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

Ensure you set the following environment variable:

```
export LO_INCLUDE_PATH=/usr/include/LibreOfficeKit
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