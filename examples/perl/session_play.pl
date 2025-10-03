# Session demo:
# run --lang perl --code "my $total = 0"
# run --lang perl --code "$total += 5"
# run --lang perl --code "$total"

use strict;
use warnings;
use feature 'say';

my $message = 'Perl sessions keep your variables around.';
say $message;
