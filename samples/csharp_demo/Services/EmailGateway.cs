namespace Demo.Services;

public class EmailGateway
{
    public void Deliver(string userId, string message)
    {
        Console.WriteLine($"EMAIL to {userId}: {message}");
    }
}
