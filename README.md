# RedoxOS init
This repository contains the init system for RedoxOS.

Init is currently being rewritten to support a couple of behaviors that are helpful for running a configurable and robust system. These include:
- Dependency relationships between services
- Parallel service startup and shutdown
- Configuration files for each service, including:
  - Dependencies
  - Methods
  - TODO: Users/Groups for services
  - TODO: Scheme namespaces for services
- Service management
  - Enable and disable services
  - Restart failed services

## How To Contribute

To learn how to contribute to this system component you need to read the following document:

- [CONTRIBUTING.md](https://gitlab.redox-os.org/redox-os/redox/-/blob/master/CONTRIBUTING.md)

## Development

To learn how to do development with this system component inside the Redox build system you need to read the [Build System](https://doc.redox-os.org/book/build-system-reference.html) and [Coding and Building](https://doc.redox-os.org/book/coding-and-building.html) pages.

### How To Build

To build this system component you need to download the Redox build system, you can learn how to do it on the [Building Redox](https://doc.redox-os.org/book/podman-build.html) page.

This is necessary because they only work with cross-compilation to a Redox virtual machine, but you can do some testing from Linux.
