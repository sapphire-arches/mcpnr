YOSYS_CONFIG ?= yosys-config
CXX ?= $(shell $(YOSYS_CONFIG) --cxx)
CXXFLAGS ?= $(shell $(YOSYS_CONFIG) --cxxflags) #-DSDC_DEBUG
LDFLAGS ?= $(shell $(YOSYS_CONFIG) --ldflags)
LDLIBS ?= $(shell $(YOSYS_CONFIG) --ldlibs)
PLUGINS_DIR ?= $(shell $(YOSYS_CONFIG) --datdir)/plugins
DATA_DIR ?= $(shell $(YOSYS_CONFIG) --datdir)
EXTRA_FLAGS ?=

SOURCES := \
    synth_mc.cc

OBJS := $(patsubst %.cc,%.o,$(SOURCES))
PLUGIN_LIBRARY := synth_mc.so

all: synth_mc.so

$(OBJS): %.o: %.cc
	$(CXX) $(CXXFLAGS) $(CPPFLAGS) $(EXTRA_FLAGS) -c -o $@ $^

$(PLUGIN_LIBRARY): $(OBJS)
	$(CXX) $(CXXFLAGS) $(LDFLAGS) -shared -o $@ $^ $(LDLIBS)

# Install logic
PREFIX ?= /usr/local
BINDIR := $(PREFIX)/bin
LIBDIR := $(PREFIX)/lib/yosys
DATDIR := $(PREFIX)/share/yosys

install: synth_mc.so
	install -D "--target-directory=$(DATDIR)/plugins" $(PLUGIN_LIBRARY)

clean:
	rm -f *.d *.o *.so

.PHONY: all clean
