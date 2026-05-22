/ Sample trading system
/ Initialise tables and utilities

/ Configuration
port:5001
sym:`AAPL`GOOG`MSFT`AMZN`META

/ Trade table
trade:([] time:`time$(); sym:`symbol$(); price:`float$(); size:`int$())

/ Quote table
quote:([] time:`time$(); sym:`symbol$(); bid:`float$(); ask:`float$())

/ Insert trade
addTrade:{[s;p;sz]
  `trade insert (.z.T;s;p;sz);
  }

/ VWAP calculation
vwap:{[t]
  select vwap:size wavg price by sym from t
  }

/ Get last price
lastPrice:{[s]
  exec last price from trade where sym=s
  }

/ Filter by time range
tradesBetween:{[t1;t2]
  select from trade where time within (t1;t2)
  }

/ Spread calculation
spread:{select sym,spread:ask-bid from quote}

/ Top N by volume
topN:{[n]
  select[n] from `size xdesc select sum size by sym from trade
  }

/ Running stats
stats:{
  select sym,
    avgPx:avg price,
    minPx:min price,
    maxPx:max price,
    totalVol:sum size,
    numTrades:count i
  by sym from trade
  }
