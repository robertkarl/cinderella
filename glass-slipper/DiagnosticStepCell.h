#import <Cocoa/Cocoa.h>

@interface DiagnosticStepCell : NSTableCellView

@property (strong) NSTextField *titleLabel;
@property (strong) NSTextField *summaryLabel;
@property (strong) NSTextField *detailLabel;
@property (strong) NSTextField *statusIndicator;
@property (strong) NSProgressIndicator *spinner;

- (void)configureWithStep:(NSDictionary *)step;

@end
