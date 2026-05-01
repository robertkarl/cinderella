#import "DiagnosticStepCell.h"

static const CGFloat kRowHeight = 88.0;
static const CGFloat kStatusWidth = 60.0;
static const CGFloat kPadding = 12.0;

@implementation DiagnosticStepCell

- (instancetype)initWithFrame:(NSRect)frameRect {
    self = [super initWithFrame:frameRect];
    if (self) {
        [self setupSubviews];
    }
    return self;
}

- (void)setupSubviews {
    CGFloat textWidth = self.bounds.size.width - kStatusWidth - kPadding * 3;

    // Title label — bold, larger
    _titleLabel = [NSTextField labelWithString:@""];
    _titleLabel.font = [NSFont boldSystemFontOfSize:15.0];
    _titleLabel.textColor = [NSColor labelColor];
    _titleLabel.frame = NSMakeRect(kPadding, kRowHeight - 30, textWidth, 20);
    _titleLabel.autoresizingMask = NSViewWidthSizable;
    [self addSubview:_titleLabel];

    // Summary label — regular
    _summaryLabel = [NSTextField labelWithString:@""];
    _summaryLabel.font = [NSFont systemFontOfSize:13.0];
    _summaryLabel.textColor = [NSColor labelColor];
    _summaryLabel.frame = NSMakeRect(kPadding, kRowHeight - 52, textWidth, 18);
    _summaryLabel.autoresizingMask = NSViewWidthSizable;
    [self addSubview:_summaryLabel];

    // Detail label — smaller, gray
    _detailLabel = [NSTextField labelWithString:@""];
    _detailLabel.font = [NSFont systemFontOfSize:11.0];
    _detailLabel.textColor = [NSColor secondaryLabelColor];
    _detailLabel.frame = NSMakeRect(kPadding, kRowHeight - 72, textWidth, 16);
    _detailLabel.lineBreakMode = NSLineBreakByTruncatingTail;
    _detailLabel.autoresizingMask = NSViewWidthSizable;
    [self addSubview:_detailLabel];

    // Status indicator — large symbol on the right
    _statusIndicator = [NSTextField labelWithString:@""];
    _statusIndicator.font = [NSFont systemFontOfSize:28.0 weight:NSFontWeightMedium];
    _statusIndicator.alignment = NSTextAlignmentCenter;
    _statusIndicator.frame = NSMakeRect(self.bounds.size.width - kStatusWidth - kPadding,
                                         (kRowHeight - 34) / 2, kStatusWidth, 34);
    _statusIndicator.autoresizingMask = NSViewMinXMargin;
    [self addSubview:_statusIndicator];

    // Spinner — shown while step is in progress
    _spinner = [[NSProgressIndicator alloc] initWithFrame:
                NSMakeRect(self.bounds.size.width - kStatusWidth / 2 - 12 - kPadding,
                           (kRowHeight - 24) / 2, 24, 24)];
    _spinner.style = NSProgressIndicatorStyleSpinning;
    _spinner.controlSize = NSControlSizeRegular;
    _spinner.autoresizingMask = NSViewMinXMargin;
    _spinner.hidden = YES;
    [self addSubview:_spinner];
}

/// Strip basic markdown formatting (**bold**, *italic*, ##headings).
static NSString *stripMarkdown(NSString *s) {
    NSMutableString *r = [s mutableCopy];
    // Strip bold markers
    [r replaceOccurrencesOfString:@"**" withString:@"" options:0 range:NSMakeRange(0, r.length)];
    // Strip heading markers at start of line
    while ([r hasPrefix:@"# "]) r = [[r substringFromIndex:2] mutableCopy];
    while ([r hasPrefix:@"## "]) r = [[r substringFromIndex:3] mutableCopy];
    while ([r hasPrefix:@"### "]) r = [[r substringFromIndex:4] mutableCopy];
    return [r stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceAndNewlineCharacterSet]];
}

- (void)configureWithStep:(NSDictionary *)step {
    NSString *title = step[@"title"] ?: step[@"step"] ?: @"Unknown";
    NSString *summary = stripMarkdown(step[@"summary"] ?: @"");
    NSString *detail = step[@"detail"] ?: @"";
    NSString *status = step[@"status"];

    self.titleLabel.stringValue = title;
    self.summaryLabel.stringValue = summary;

    // Show first non-empty line of detail, stripped of markdown
    NSString *detailFirstLine = @"";
    for (NSString *line in [detail componentsSeparatedByString:@"\n"]) {
        NSString *stripped = stripMarkdown(line);
        if (stripped.length > 0 && ![stripped isEqualToString:summary]) {
            detailFirstLine = stripped;
            break;
        }
    }
    self.detailLabel.stringValue = detailFirstLine;

    if (status == nil) {
        // In progress
        self.statusIndicator.hidden = YES;
        self.spinner.hidden = NO;
        [self.spinner startAnimation:nil];
    } else if ([status isEqualToString:@"pass"]) {
        self.statusIndicator.stringValue = @"\u2713"; // checkmark
        self.statusIndicator.textColor = [NSColor systemGreenColor];
        self.statusIndicator.hidden = NO;
        self.spinner.hidden = YES;
        [self.spinner stopAnimation:nil];
    } else if ([status isEqualToString:@"fail"]) {
        self.statusIndicator.stringValue = @"!";
        self.statusIndicator.textColor = [NSColor systemRedColor];
        self.statusIndicator.hidden = NO;
        self.spinner.hidden = YES;
        [self.spinner stopAnimation:nil];
    } else if ([status isEqualToString:@"warn"]) {
        self.statusIndicator.stringValue = @"!";
        self.statusIndicator.textColor = [NSColor systemOrangeColor];
        self.statusIndicator.hidden = NO;
        self.spinner.hidden = YES;
        [self.spinner stopAnimation:nil];
    } else {
        // Unknown status — show as pass
        self.statusIndicator.stringValue = @"\u25CF"; // filled circle
        self.statusIndicator.textColor = [NSColor secondaryLabelColor];
        self.statusIndicator.hidden = NO;
        self.spinner.hidden = YES;
        [self.spinner stopAnimation:nil];
    }
}

@end
