# Office convert server 

Simple server for converting office file formats into PDF files

## Requirements

Debian:
```
sudo apt-get install libreoffice libreofficekit-dev clang
```
Fedora:
```sh
sudo dnf install libreoffice libreoffice-sdk libreofficekit-devel clang
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