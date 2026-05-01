CC = clang
CFLAGS = -fobjc-arc -Wall -Wextra -Wno-unused-parameter
FRAMEWORKS = -framework Cocoa
SOURCES = main.m DiagnosticStepCell.m
HEADERS = AppDelegate.h DiagnosticStepCell.h
TARGET = GlassSlipper

APP_BUNDLE = $(TARGET).app
APP_CONTENTS = $(APP_BUNDLE)/Contents
APP_MACOS = $(APP_CONTENTS)/MacOS

.PHONY: all clean run

all: $(APP_BUNDLE)

$(TARGET): $(SOURCES) $(HEADERS)
	$(CC) $(CFLAGS) $(FRAMEWORKS) $(SOURCES) -o $(TARGET)

$(APP_BUNDLE): $(TARGET) Info.plist
	mkdir -p $(APP_MACOS)
	cp $(TARGET) $(APP_MACOS)/
	cp Info.plist $(APP_CONTENTS)/
	@echo "Built $(APP_BUNDLE)"

run: $(APP_BUNDLE)
	open $(APP_BUNDLE)

clean:
	rm -rf $(TARGET) $(APP_BUNDLE)
