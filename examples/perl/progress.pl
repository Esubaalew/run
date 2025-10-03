# Expected output:
# step 1
# step 2
# step 3

use strict;
use warnings;
use feature 'say';

foreach my $step (1 .. 3) {
    say "step $step";
}
