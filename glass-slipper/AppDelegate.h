#import <Cocoa/Cocoa.h>

@interface AppDelegate : NSObject <NSApplicationDelegate, NSTableViewDataSource, NSTableViewDelegate>

@property (strong) NSWindow *window;
@property (strong) NSTextField *urlField;
@property (strong) NSButton *diagnoseButton;
@property (strong) NSTableView *tableView;
@property (strong) NSScrollView *scrollView;

@property (strong) NSTask *task;
@property (strong) NSPipe *stdoutPipe;
@property (strong) NSPipe *stderrPipe;
@property (strong) NSMutableData *lineBuffer;
@property (strong) NSMutableData *stderrBuffer;

@property (strong) NSMutableArray *steps;       // Array of NSMutableDictionary
@property (strong) NSMutableDictionary *stepIndex; // step_id -> index in steps array

@property (assign) BOOL isRunning;
@property (strong) NSFileHandle *logFileHandle;

@end
