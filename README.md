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
