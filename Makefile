# Re-Flora Makefile — platform dispatcher
#
# Delegates to a platform-specific Makefile based on the detected OS.
# Supported: macOS (Darwin), Linux, Windows (MSYS2/MinGW/Git Bash)

UNAME_S := $(shell uname -s)

ifeq ($(UNAME_S),Darwin)
    PLATFORM_MAKEFILE := Makefile.macos
else ifeq ($(UNAME_S),Linux)
    PLATFORM_MAKEFILE := Makefile.linux
else ifneq (,$(findstring MINGW,$(UNAME_S)))
    PLATFORM_MAKEFILE := Makefile.windows
else ifneq (,$(findstring MSYS,$(UNAME_S)))
    PLATFORM_MAKEFILE := Makefile.windows
else
    PLATFORM_MAKEFILE :=
endif

ifdef PLATFORM_MAKEFILE

# Forward all targets to the platform-specific Makefile
%:
	@$(MAKE) -f $(PLATFORM_MAKEFILE) $@

# Explicit default so bare `make` works
.DEFAULT_GOAL := all
all:
	@$(MAKE) -f $(PLATFORM_MAKEFILE) all

else

# Unsupported platform — print a helpful message for every target
%:
	$(error No Makefile for platform '$(UNAME_S)'. Supported: Darwin (macOS), Linux, MINGW/MSYS (Windows). Create a Makefile.$(UNAME_S) to add support.)

all:
	$(error No Makefile for platform '$(UNAME_S)'. Supported: Darwin (macOS), Linux, MINGW/MSYS (Windows). Create a Makefile.$(UNAME_S) to add support.)

endif

.PHONY: all
