package com.example.counter;

import static org.junit.Assert.assertEquals;

import org.junit.Test;

public class CounterTest {
    @Test
    public void startsAtZero() {
        assertEquals(0, new Counter().value());
    }

    @Test
    public void incrementsByOne() {
        Counter counter = new Counter();
        counter.increment();
        assertEquals(1, counter.value());
    }

    @Test
    public void neverGoesBelowZero() {
        Counter counter = new Counter();
        counter.decrement();
        assertEquals(0, counter.value());
    }
}
