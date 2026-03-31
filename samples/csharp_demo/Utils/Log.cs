namespace Demo.Utils;

public static class Log
{
    public static void WriteInfo(string message)
    {
        Console.WriteLine($"[INFO] {message}");
    }

    public static void WriteError(string message)
    {
        Console.Error.WriteLine($"[ERROR] {message}");
    }
}
