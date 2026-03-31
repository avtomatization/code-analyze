package com.example.service;

import com.example.util.LoggerUtil;

public class NotificationService {
    private final EmailGateway emailGateway = new EmailGateway();

    public void sendPaymentStatus(String userId, boolean paid) {
        String status = paid ? "PAID" : "FAILED";
        LoggerUtil.info("Sending payment status " + status + " for " + userId);
        emailGateway.deliver(userId, "Payment status: " + status);
    }
}
