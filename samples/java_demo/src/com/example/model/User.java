package com.example.model;

public class User {
    private final String id;
    private double balance;

    public User(String id, double balance) {
        this.id = id;
        this.balance = balance;
    }

    public boolean credit(double amount) {
        if (amount <= balance) {
            balance -= amount;
            return true;
        }
        return false;
    }

    public String getId() {
        return id;
    }
}
