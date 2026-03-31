package com.example.service;

public class ProjectCoordinator {
    private final BillingService billingService;
    private final NotificationService notificationService;

    public ProjectCoordinator() {
        this.billingService = new BillingService();
        this.notificationService = new NotificationService();
    }

    public void run(String userId, double amount) {
        boolean paid = billingService.chargeUser(userId, amount);
        notificationService.sendPaymentStatus(userId, paid);
    }
}
