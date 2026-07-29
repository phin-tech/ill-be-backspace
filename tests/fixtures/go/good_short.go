package main

// Buffered so a slow consumer cannot stall the producer.
var ch = make(chan int, 64)
