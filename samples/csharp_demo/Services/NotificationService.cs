using Demo.Utils;

namespace Demo.Services;

public class NotificationService
{
    private readonly EmailGateway _emailGateway = new();

    public void SendPaymentStatus(string userId, bool paid)
    {
        var status = paid ? "PAID" : "FAILED";
        Log.WriteInfo($"Sending payment status {status} for {userId}");
        _emailGateway.Deliver(userId, $"Payment status: {status}");
    }
}
