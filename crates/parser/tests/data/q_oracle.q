/ One expression per line, each parsed on its own; compared with q 4.1 `parse` by tests/q_oracle.rs.
1+2
f x
a:1
f[a;b]
f[]
(a;b)
(1;)
x[1;]
f g h x
1 2+3
enlist 1
x in y
x f y
`a `b
f/[x;y]
@[f;x;:;y]
x .y
x -1
x-1
a(b)c
f'[x;y]
x+/y
$[a;b;c]
'[f;g]
-1
+/
a+:1
1+
(+)
x,'y
x,/:y
(f/)x
f\[x;y]
{x+y}/[1 2 3]
{x+y}[1;2]
{x+y}[1]2
f[x]y
f[x][y]
f[x]'[y]
a[b][c]d
(a b)c
a (b c) d
a:b:c
a[i]:v
a[i;j]:v
a[i]+:v
(a;b):(1;2)
x:(y:2)+z
(+/)x
(+/) x
+/[x]
x+/[y]
(1 2;3 4)
((1;2);(3;4))
f@g x
f . (x;y)
x@'y
x#/:y
(,/)x
,/[x]
raze each x
x {x+y}/ y
@[x;where y;:;z]
.[f;(x;y);{x}]
$[a;b;$[c;d;e]]
$[a;[b;c];d]
if[a;b;c]
do[3;x+:1]
while[x<3;x+:1]
'"err"
x:'y
`a`b!1 2
(`a`b!1 2)`a
d[`a]
d`a
t[`col]
f[;1]
f[1;]
f[;]
(;1)
1 2 3[1]
"abc"[0]
"abc" 0
`a`b`c 1
x[y;z][w]
f/:[x;y]
f\:[x;y]
f':[x]
f':x
x f':y
0:[x;y]
x 0: y
x!y
!x
x?y
x _ y
-x
#x
neg x
neg neg x
not null x
x=y
x<>y
x<=y
x>=y
a and b or c
a mod b
x xbar y
(x)y
(x;y)z
{[a;b]a+b}[1;2]
{x}'[x]
f:{[x]x+1}
.a.b:1
.a.b[c]:1
f[g[h[x]]]
f[g[x];h[y]]
((f g) h) x
f (g (h x))
x+y*z-w
(x+y)*z
x*(y+z)-w
x+y/z
x+/y,z
1 2 3+4 5 6*7
`a`b`c?`b
x?`b
(),x
(),/x
x,()
(1 2;`a`b;"ab")
[a;b]
x[;1;]
x[::]
x::1
::
(::)
f ::
x . y
x .z.p
.z.p x
.q.til 3
til 3
-3!x
-3! x
0N!x
x - 1
x -1 2
f -1
1 -1
(-1)
(- 1)
-:[x]
neg[x]
-[x;y]
x-[y]
x-'y
x-/:y
f each x
x f/ y
f/ x
(f/) x
(f/)[x;y]
f/:x
/ Composition: an argument or right operand that ends in a verb composes.
g f+
x+y+
f g h+
x in +
x,:+
g @+
x y+
f x:y+
neg x+
enlist f+
g x f/
g x each
(f g+;1)
f[g+]
g (+)
g (x+)
f:+/
a:b+
f::+
x f/
(-) x
