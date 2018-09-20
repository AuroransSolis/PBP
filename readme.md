This is my attempt at a first distributed computing project. It's probably not the best choice as it's an 
embarrassingly parallel problem, but oh well.

The problem this project is "trying" to solve is finding a 3x3 magic square of perfect squares. For the 
uninitiated, a magic square is a grid of numbers where the sums of the numbers in each row, each solumn, and 
the two long diagonals are equal. So in this square:
```
a  b  c
d  e  f
g  h  i
```
it is magic if
```
a + b + c = S
d + e + f = S
g + h + i = S
a + d + g = S
b + e + h = S
c + f + i = S
a + e + i = S
d + e + g = S
```
where `S` is the "magic sum." Now, try and make one out of perfect squares. No, seriously. Give it a go. It's
hard as frig, lads. So I wrote this program to try and find one.

In the code I make use of a couple tricks that I found on a webpage dedicated to this problem. I can't be 
bothered to link it, so search for "3x3 magic square of perfect squares MultiMagie" or something similar if 
you want to find out more about this problem.


An explanation of what I'm trying to get this program to do can be found in `PBP explanation.txt`. That is
how I want this program to function, but it is not how it is functioning currently.

To run this program, first open a terminal and do `cargo run --release --bin host`, then open another and
do `cargo run --release --bin client`. Unfortunately, the `--release` is not optional, since the
`is_valid_square()` function in both the host and client make use of overflow as part of testing whether or
not the input is a perfect square. This causes panics in debug builds.

Other things:
- the number of threads spawned by each client is a `const` value in  
`client.rs`. Feel free to change this to suit whatever machine you're 
running on.
- The maximum `X` value is a `const` value in `host.rs`. Feel free to 
change this to suit whatever you want. [DWTFYW license pending?]
