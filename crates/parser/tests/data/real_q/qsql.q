/ qSQL queries
select from trade
select sym,price from trade
select price,size from trade where sym=`AAPL
select avg price by sym from trade
update price:price*1.1 from trade where sym=`AAPL
exec price from trade where sym=`GOOG
delete from trade where price<100
