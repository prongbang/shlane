package com.example.counter;

/** The smallest thing worth testing. */
public final class Counter {
    private int value;

    public int value() {
        return value;
    }

    public void increment() {
        value++;
    }

    public void decrement() {
        if (value > 0) {
            value--;
        }
    }
}
