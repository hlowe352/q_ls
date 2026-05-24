q1a: {[]
    input: read0 `:q1a.txt;
    path: {(x+$["L"=first y;neg;] "J"$1_ y) mod 100}\[50;input];
    sum path=0
  };

show q1a[];

// Not 6829, less
q1b: {[]
    input: read0 `:q1a.txt;
    path: {loc: x[0]+$[minus: "L"=first y;neg;] "J"$1_ y; (pos;$[(not minus) and 0=x 0;1+;] abs loc div 100;0=pos: loc mod 100)}\[(50;0;0);input];
    sum path[;1]
  };

show q1b[];
