package com.example.service;

import com.example.model.User;
import com.example.util.LoggerUtil;

public class BillingService {
    private final UserRepository userRepository = new UserRepository();

    public boolean chargeUser(String userId, double amount) {
        LoggerUtil.info("Charge requested for " + userId + " amount " + amount);
        User user = userRepository.findById(userId);
        if (user == null) {
            LoggerUtil.error("User not found: " + userId);
            return false;
        }
        return user.credit(amount);
    }
}
