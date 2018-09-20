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
Where
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
Where `S` is the "magic sum." Now, try and make one out of perfect squares. No, seriously. Give it a go. It's 
hard as frig, lads. So I wrote this program to try and find one.

In the code I make use of a couple tricks that I found on a webpage dedicated to this problem. I can't be 
bothered to link it, so search for "3x3 magic square of perfect squares MultiMagie" or something similar if 
you want to find out more about this problem.


An exmplanation of what I'm trying to get this program to do can be found in `PBP explanation.txt`. That is 
how I want this program to function, but not how it is functioning currently.
