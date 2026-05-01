#import <Cocoa/Cocoa.h>
#import <signal.h>
#import "AppDelegate.h"
#import "DiagnosticStepCell.h"

static const CGFloat kRowHeight = 88.0;

@implementation AppDelegate

- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    _steps = [NSMutableArray array];
    _stepIndex = [NSMutableDictionary dictionary];
    _lineBuffer = [NSMutableData data];
    _stderrBuffer = [NSMutableData data];
    _isRunning = NO;

    [self setupWindow];
    [self setupUI];
}

- (void)setupWindow {
    NSRect frame = NSMakeRect(200, 200, 640, 520);
    NSWindowStyleMask style = NSWindowStyleMaskTitled
        | NSWindowStyleMaskClosable
        | NSWindowStyleMaskMiniaturizable
        | NSWindowStyleMaskResizable;
    _window = [[NSWindow alloc] initWithContentRect:frame
                                          styleMask:style
                                            backing:NSBackingStoreBuffered
                                              defer:NO];
    _window.title = @"Glass Slipper";
    _window.minSize = NSMakeSize(480, 360);
    [_window center];
}

- (void)setupUI {
    NSView *contentView = _window.contentView;

    // Top bar: URL field + Diagnose button
    CGFloat topY = contentView.bounds.size.height - 48;

    _urlField = [[NSTextField alloc] initWithFrame:NSMakeRect(12, topY, 480, 28)];
    _urlField.placeholderString = @"http://localhost:14094";
    _urlField.autoresizingMask = NSViewWidthSizable | NSViewMinYMargin;
    _urlField.bezelStyle = NSTextFieldRoundedBezel;
    [contentView addSubview:_urlField];

    _diagnoseButton = [[NSButton alloc] initWithFrame:
                        NSMakeRect(contentView.bounds.size.width - 112, topY, 100, 28)];
    _diagnoseButton.title = @"Diagnose";
    _diagnoseButton.bezelStyle = NSBezelStyleRounded;
    _diagnoseButton.target = self;
    _diagnoseButton.action = @selector(diagnoseClicked:);
    _diagnoseButton.autoresizingMask = NSViewMinXMargin | NSViewMinYMargin;
    [contentView addSubview:_diagnoseButton];

    // Table view in scroll view
    _scrollView = [[NSScrollView alloc] initWithFrame:
                    NSMakeRect(0, 0, contentView.bounds.size.width, topY - 12)];
    _scrollView.hasVerticalScroller = YES;
    _scrollView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    _scrollView.borderType = NSNoBorder;

    _tableView = [[NSTableView alloc] initWithFrame:_scrollView.bounds];
    _tableView.dataSource = self;
    _tableView.delegate = self;
    _tableView.headerView = nil; // No column headers
    _tableView.rowHeight = kRowHeight;
    _tableView.selectionHighlightStyle = NSTableViewSelectionHighlightStyleRegular;
    _tableView.usesAlternatingRowBackgroundColors = YES;

    NSTableColumn *col = [[NSTableColumn alloc] initWithIdentifier:@"step"];
    col.width = _scrollView.bounds.size.width;
    col.resizingMask = NSTableColumnAutoresizingMask;
    [_tableView addTableColumn:col];

    _scrollView.documentView = _tableView;
    [contentView addSubview:_scrollView];

    [_window makeKeyAndOrderFront:nil];
}

#pragma mark - Actions

- (void)diagnoseClicked:(id)sender {
    if (_isRunning) {
        [self stopDiagnosis];
        return;
    }
    [self startDiagnosis];
}

- (void)startDiagnosis {
    NSString *url = _urlField.stringValue;
    if (url.length == 0) {
        url = @"http://localhost:14094";
        _urlField.stringValue = url;
    }

    // Clear previous results
    [_steps removeAllObjects];
    [_stepIndex removeAllObjects];
    [_lineBuffer setLength:0];
    [_stderrBuffer setLength:0];
    [_tableView reloadData];

    // Open log file for tail -f
    NSString *logPath = [NSTemporaryDirectory() stringByAppendingPathComponent:@"glass-slipper.jsonl"];
    [[NSFileManager defaultManager] createFileAtPath:logPath contents:nil attributes:nil];
    _logFileHandle = [NSFileHandle fileHandleForWritingAtPath:logPath];
    [_logFileHandle truncateFileAtOffset:0];
    NSLog(@"Glass Slipper log: %@", logPath);

    // Find cinderella binary
    NSString *cinderellaPath = [self findCinderella];
    if (!cinderellaPath) {
        NSAlert *alert = [[NSAlert alloc] init];
        alert.messageText = @"cinderella not found";
        alert.informativeText = @"Build cinderella first: cargo build --release\n"
                                 "Then ensure it's in your PATH or next to this app.";
        alert.alertStyle = NSAlertStyleWarning;
        [alert runModal];
        return;
    }

    // Build prompt from URL
    NSString *prompt = [NSString stringWithFormat:@"Diagnose this URL: %@", url];

    // Launch NSTask — argument array, no shell
    _task = [[NSTask alloc] init];
    _task.executableURL = [NSURL fileURLWithPath:cinderellaPath];
    // Expand ~ to home directory for model path
    NSString *home = NSHomeDirectory();
    NSString *modelPath = [home stringByAppendingPathComponent:@"models/Qwen3.5-9B-Q5_K_M.gguf"];
    _task.arguments = @[@".", @"-p", prompt, @"--playbook", @"network-debug", @"--format", @"json",
                        @"--model", modelPath];

    // stdout pipe
    _stdoutPipe = [NSPipe pipe];
    _task.standardOutput = _stdoutPipe;

    // stderr pipe
    _stderrPipe = [NSPipe pipe];
    _task.standardError = _stderrPipe;

    // Read stdout in background
    NSFileHandle *stdoutHandle = _stdoutPipe.fileHandleForReading;
    [[NSNotificationCenter defaultCenter] addObserver:self
                                             selector:@selector(stdoutDataAvailable:)
                                                 name:NSFileHandleDataAvailableNotification
                                               object:stdoutHandle];
    [stdoutHandle waitForDataInBackgroundAndNotify];

    // Read stderr in background
    NSFileHandle *stderrHandle = _stderrPipe.fileHandleForReading;
    [[NSNotificationCenter defaultCenter] addObserver:self
                                             selector:@selector(stderrDataAvailable:)
                                                 name:NSFileHandleDataAvailableNotification
                                               object:stderrHandle];
    [stderrHandle waitForDataInBackgroundAndNotify];

    // Set termination handler
    __weak typeof(self) weakSelf = self;
    _task.terminationHandler = ^(NSTask *t) {
        dispatch_async(dispatch_get_main_queue(), ^{
            [weakSelf taskDidTerminate:t];
        });
    };

    // Launch
    NSError *error = nil;
    if (![_task launchAndReturnError:&error]) {
        NSAlert *alert = [[NSAlert alloc] init];
        alert.messageText = @"Failed to launch cinderella";
        alert.informativeText = error.localizedDescription;
        alert.alertStyle = NSAlertStyleWarning;
        [alert runModal];
        return;
    }

    _isRunning = YES;
    _diagnoseButton.title = @"Stop";
}

- (void)stopDiagnosis {
    if (_task && _task.isRunning) {
        [_task terminate];
        // SIGKILL fallback if SIGTERM doesn't work within 3 seconds
        pid_t pid = _task.processIdentifier;
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(3.0 * NSEC_PER_SEC)),
                       dispatch_get_main_queue(), ^{
            if (self->_task && self->_task.isRunning) {
                NSLog(@"Glass Slipper: SIGTERM ignored, sending SIGKILL to pid %d", pid);
                kill(pid, SIGKILL);
            }
        });
    }
}

- (NSString *)findCinderella {
    // Check next to this app
    NSString *appDir = [[NSBundle mainBundle] bundlePath];
    NSString *adjacent = [[appDir stringByDeletingLastPathComponent]
                          stringByAppendingPathComponent:@"cinderella"];
    if ([[NSFileManager defaultManager] isExecutableFileAtPath:adjacent]) {
        return adjacent;
    }

    // Check cargo build output
    NSString *cargoDebug = [[[appDir stringByDeletingLastPathComponent]
                             stringByAppendingPathComponent:@"../target/debug/cinderella"]
                            stringByStandardizingPath];
    if ([[NSFileManager defaultManager] isExecutableFileAtPath:cargoDebug]) {
        return cargoDebug;
    }

    NSString *cargoRelease = [[[appDir stringByDeletingLastPathComponent]
                               stringByAppendingPathComponent:@"../target/release/cinderella"]
                              stringByStandardizingPath];
    if ([[NSFileManager defaultManager] isExecutableFileAtPath:cargoRelease]) {
        return cargoRelease;
    }

    // Check PATH via which
    NSTask *which = [[NSTask alloc] init];
    which.executableURL = [NSURL fileURLWithPath:@"/usr/bin/which"];
    which.arguments = @[@"cinderella"];
    NSPipe *whichPipe = [NSPipe pipe];
    which.standardOutput = whichPipe;
    which.standardError = [NSPipe pipe];
    NSError *err = nil;
    [which launchAndReturnError:&err];
    if (!err) {
        [which waitUntilExit];
        NSData *data = [whichPipe.fileHandleForReading readDataToEndOfFile];
        NSString *path = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
        path = [path stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceAndNewlineCharacterSet]];
        if (path.length > 0 && [[NSFileManager defaultManager] isExecutableFileAtPath:path]) {
            return path;
        }
    }

    return nil;
}

#pragma mark - NSFileHandle notifications

- (void)stdoutDataAvailable:(NSNotification *)notification {
    NSFileHandle *handle = notification.object;
    NSData *data = [handle availableData];

    if (data.length == 0) {
        // EOF
        return;
    }

    // Accumulate in line buffer and process complete lines
    [_lineBuffer appendData:data];
    [self processLineBuffer];

    // Continue reading
    [handle waitForDataInBackgroundAndNotify];
}

- (void)stderrDataAvailable:(NSNotification *)notification {
    NSFileHandle *handle = notification.object;
    NSData *data = [handle availableData];
    if (data.length > 0) {
        [_stderrBuffer appendData:data];
        [handle waitForDataInBackgroundAndNotify];
    }
}

- (void)processLineBuffer {
    // Split on newlines, process each complete line
    NSString *bufStr = [[NSString alloc] initWithData:_lineBuffer encoding:NSUTF8StringEncoding];
    if (!bufStr) {
        NSLog(@"Glass Slipper: UTF-8 decode failed (%lu bytes), clearing buffer", (unsigned long)_lineBuffer.length);
        [_lineBuffer setLength:0];
        return;
    }

    NSArray *lines = [bufStr componentsSeparatedByString:@"\n"];
    if (lines.count <= 1) {
        // No complete line yet
        return;
    }

    // Process all complete lines (everything except the last fragment)
    for (NSUInteger i = 0; i < lines.count - 1; i++) {
        NSString *line = lines[i];
        if (line.length > 0) {
            [self processJSONLine:line];
        }
    }

    // Keep the last (incomplete) fragment in the buffer
    NSString *remainder = [lines lastObject];
    _lineBuffer = [[remainder dataUsingEncoding:NSUTF8StringEncoding] mutableCopy] ?: [NSMutableData data];
}

- (void)processJSONLine:(NSString *)line {
    // Tee to log file for tail -f
    if (_logFileHandle) {
        NSString *logged = [line stringByAppendingString:@"\n"];
        [_logFileHandle writeData:[logged dataUsingEncoding:NSUTF8StringEncoding]];
        [_logFileHandle synchronizeFile];
    }

    NSData *jsonData = [line dataUsingEncoding:NSUTF8StringEncoding];
    NSError *error = nil;
    NSDictionary *event = [NSJSONSerialization JSONObjectWithData:jsonData
                                                         options:0
                                                           error:&error];
    if (error || ![event isKindOfClass:[NSDictionary class]]) {
        NSLog(@"Glass Slipper: invalid JSON line: %@", line);
        return;
    }

    NSString *eventType = event[@"event"];
    if (!eventType) return;

    dispatch_async(dispatch_get_main_queue(), ^{
        [self handleEvent:event type:eventType];
    });
}

- (void)handleEvent:(NSDictionary *)event type:(NSString *)eventType {
    if ([eventType isEqualToString:@"step_start"]) {
        NSString *stepId = event[@"step"] ?: @"unknown";
        NSString *title = event[@"title"] ?: stepId;

        NSMutableDictionary *stepData = [NSMutableDictionary dictionaryWithDictionary:@{
            @"step": stepId,
            @"title": title,
        }];

        NSNumber *idx = @(_steps.count);
        [_steps addObject:stepData];
        _stepIndex[stepId] = idx;

        [_tableView insertRowsAtIndexes:[NSIndexSet indexSetWithIndex:idx.unsignedIntegerValue]
                          withAnimation:NSTableViewAnimationSlideDown];

        // Scroll to bottom
        [_tableView scrollRowToVisible:_steps.count - 1];

    } else if ([eventType isEqualToString:@"step_complete"]) {
        NSString *stepId = event[@"step"] ?: @"unknown";
        NSNumber *idx = _stepIndex[stepId];
        if (idx) {
            NSMutableDictionary *stepData = _steps[idx.unsignedIntegerValue];
            stepData[@"status"] = event[@"status"] ?: @"pass";
            stepData[@"summary"] = event[@"summary"] ?: @"";
            stepData[@"detail"] = event[@"detail"] ?: @"";

            NSIndexSet *rowSet = [NSIndexSet indexSetWithIndex:idx.unsignedIntegerValue];
            NSIndexSet *colSet = [NSIndexSet indexSetWithIndex:0];
            [_tableView reloadDataForRowIndexes:rowSet columnIndexes:colSet];
        }

    } else if ([eventType isEqualToString:@"text"]) {
        // Update detail on current step if any
        if (_steps.count > 0) {
            NSMutableDictionary *current = [_steps lastObject];
            if (!current[@"status"]) { // only if step is still in progress
                NSString *existing = current[@"detail"] ?: @"";
                NSString *content = event[@"content"] ?: @"";
                current[@"detail"] = [existing stringByAppendingString:content];

                // Update summary to first line of accumulated text
                NSString *fullText = current[@"detail"];
                NSString *firstLine = [[fullText componentsSeparatedByString:@"\n"] firstObject];
                if (firstLine.length > 0 && ((NSString *)current[@"summary"]).length == 0) {
                    current[@"summary"] = firstLine;
                }

                NSUInteger idx = _steps.count - 1;
                [_tableView reloadDataForRowIndexes:[NSIndexSet indexSetWithIndex:idx]
                                      columnIndexes:[NSIndexSet indexSetWithIndex:0]];
            }
        }

    } else if ([eventType isEqualToString:@"tool_start"]) {
        // Could update detail, but step_complete will overwrite — skip for cleanliness

    } else if ([eventType isEqualToString:@"tool_done"]) {
        // Handled by step_complete

    } else if ([eventType isEqualToString:@"done"]) {
        // Diagnosis complete
        _isRunning = NO;
        _diagnoseButton.title = @"Diagnose";

    } else if ([eventType isEqualToString:@"warning"]) {
        NSLog(@"Glass Slipper warning: %@", event[@"message"]);
    }
}

- (void)taskDidTerminate:(NSTask *)task {
    // NOTE: There's a potential race here — if this handler fires before the final
    // stdoutDataAvailable: notification, removeObserver drops the last chunk of stdout
    // (containing final step_complete/done events). In practice NSFileHandle notifications
    // are delivered before the termination handler on the main queue. If the last row
    // keeps its spinner during testing, drain the pipe here before removing observers.
    [[NSNotificationCenter defaultCenter] removeObserver:self
                                                    name:NSFileHandleDataAvailableNotification
                                                  object:nil];
    _isRunning = NO;
    _diagnoseButton.title = @"Diagnose";

    if (task.terminationStatus != 0) {
        // Show error from stderr
        NSString *stderrText = [[NSString alloc] initWithData:_stderrBuffer
                                                     encoding:NSUTF8StringEncoding] ?: @"Unknown error";
        stderrText = [stderrText stringByTrimmingCharactersInSet:
                      [NSCharacterSet whitespaceAndNewlineCharacterSet]];

        if (stderrText.length == 0) {
            stderrText = [NSString stringWithFormat:@"cinderella exited with code %d",
                          task.terminationStatus];
        }

        // Add an error row
        NSMutableDictionary *errorStep = [NSMutableDictionary dictionaryWithDictionary:@{
            @"step": @"error",
            @"title": @"Error",
            @"summary": stderrText,
            @"detail": stderrText,
            @"status": @"fail",
        }];
        [_steps addObject:errorStep];
        [_tableView insertRowsAtIndexes:[NSIndexSet indexSetWithIndex:_steps.count - 1]
                          withAnimation:NSTableViewAnimationSlideDown];
        [_tableView scrollRowToVisible:_steps.count - 1];
    }
}

#pragma mark - NSTableViewDataSource

- (NSInteger)numberOfRowsInTableView:(NSTableView *)tableView {
    return _steps.count;
}

#pragma mark - NSTableViewDelegate

- (NSView *)tableView:(NSTableView *)tableView viewForTableColumn:(NSTableColumn *)tableColumn row:(NSInteger)row {
    DiagnosticStepCell *cell = [tableView makeViewWithIdentifier:@"StepCell" owner:self];
    if (!cell) {
        cell = [[DiagnosticStepCell alloc] initWithFrame:NSMakeRect(0, 0, tableView.bounds.size.width, kRowHeight)];
        cell.identifier = @"StepCell";
    }
    [cell configureWithStep:_steps[row]];
    return cell;
}

- (CGFloat)tableView:(NSTableView *)tableView heightOfRow:(NSInteger)row {
    return kRowHeight;
}

- (void)tableViewSelectionDidChange:(NSNotification *)notification {
    NSInteger row = _tableView.selectedRow;
    if (row < 0 || (NSUInteger)row >= _steps.count) return;

    NSDictionary *step = _steps[row];
    NSString *detail = step[@"detail"] ?: step[@"summary"] ?: @"";
    if (detail.length == 0) return;

    // Copy to clipboard
    NSPasteboard *pb = [NSPasteboard generalPasteboard];
    [pb clearContents];
    [pb setString:detail forType:NSPasteboardTypeString];

    // Brief visual feedback — flash the title
    // Deselect after a moment so it doesn't stay highlighted
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.3 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        [self->_tableView deselectAll:nil];
    });
}

- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender {
    return YES;
}

- (void)applicationWillTerminate:(NSNotification *)notification {
    if (_task && _task.isRunning) {
        [_task terminate];
    }
}

@end

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        NSApplication *app = [NSApplication sharedApplication];
        app.activationPolicy = NSApplicationActivationPolicyRegular;

        AppDelegate *delegate = [[AppDelegate alloc] init];
        app.delegate = delegate;

        // Create a basic menu bar
        NSMenu *menubar = [[NSMenu alloc] init];
        NSMenuItem *appMenuItem = [[NSMenuItem alloc] init];
        [menubar addItem:appMenuItem];
        app.mainMenu = menubar;

        NSMenu *appMenu = [[NSMenu alloc] init];
        [appMenu addItemWithTitle:@"Quit Glass Slipper"
                           action:@selector(terminate:)
                    keyEquivalent:@"q"];
        appMenuItem.submenu = appMenu;

        // Edit menu — required for Cmd+C/V/X/A to work in text fields
        NSMenuItem *editMenuItem = [[NSMenuItem alloc] init];
        [menubar addItem:editMenuItem];
        NSMenu *editMenu = [[NSMenu alloc] initWithTitle:@"Edit"];
        [editMenu addItemWithTitle:@"Cut" action:@selector(cut:) keyEquivalent:@"x"];
        [editMenu addItemWithTitle:@"Copy" action:@selector(copy:) keyEquivalent:@"c"];
        [editMenu addItemWithTitle:@"Paste" action:@selector(paste:) keyEquivalent:@"v"];
        [editMenu addItemWithTitle:@"Select All" action:@selector(selectAll:) keyEquivalent:@"a"];
        editMenuItem.submenu = editMenu;

        [app activateIgnoringOtherApps:YES];
        [app run];
    }
    return 0;
}
