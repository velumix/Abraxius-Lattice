using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;

namespace Abraxius.Lattice.Services;

public enum ShortcutInstallStatus
{
    Installed,
    AlreadyInstalled,
    Unavailable,
    Failed,
}

public sealed record ShortcutInstallResult(
    ShortcutInstallStatus Status,
    string? Location,
    string? Detail);

/// <summary>
/// Installs an idempotent launcher in the user's desktop/application menu.
/// It never installs to a system-wide location and never elevates privileges.
/// </summary>
public static class DesktopShortcutInstaller
{
    public static ShortcutInstallResult Install()
    {
        try
        {
            return OperatingSystem.IsLinux()
                ? InstallLinux()
                : OperatingSystem.IsWindows()
                    ? InstallWindows()
                    : OperatingSystem.IsMacOS()
                        ? InstallMacOs()
                        : new(ShortcutInstallStatus.Unavailable, null, "Unsupported desktop platform.");
        }
        catch (Exception exception)
        {
            return new(ShortcutInstallStatus.Failed, null, exception.Message);
        }
    }

    [SupportedOSPlatform("linux")]
    private static ShortcutInstallResult InstallLinux()
    {
        var dataHome = Environment.GetEnvironmentVariable("XDG_DATA_HOME");
        if (string.IsNullOrWhiteSpace(dataHome))
        {
            dataHome = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".local", "share");
        }

        var applicationsDirectory = Path.Combine(dataHome, "applications");
        var menuPath = Path.Combine(applicationsDirectory, "abraxius-lattice.desktop");
        var pngPath = Path.Combine(AppContext.BaseDirectory, "Assets", "Lattice.png");

        Directory.CreateDirectory(applicationsDirectory);
        var iconDirectory = Path.Combine(dataHome, "icons", "hicolor", "128x128", "apps");
        var iconPath = InstallLinuxIcon(pngPath, iconDirectory);

        var contents = BuildDesktopEntry(iconPath);
        WriteTextAtomically(menuPath, contents);
        MarkExecutable(menuPath);

        var desktopDirectory = ResolveDesktopDirectory();
        var desktopPath = desktopDirectory is not null
            ? Path.Combine(desktopDirectory, "Abraxius Lattice.desktop")
            : null;
        if (desktopPath is not null)
        {
            WriteTextAtomically(desktopPath, contents);
            MarkExecutable(desktopPath);
        }

        RefreshLinuxDesktopCaches(dataHome, applicationsDirectory, iconDirectory);

        return new(
            ShortcutInstallStatus.Installed,
            desktopPath ?? menuPath,
            desktopPath is null ? "Application menu entry installed; no desktop directory was available." : "Desktop and application menu entries installed.");
    }

    [SupportedOSPlatform("windows")]
    private static ShortcutInstallResult InstallWindows()
    {
        var desktopDirectory = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
        if (string.IsNullOrWhiteSpace(desktopDirectory))
        {
            return new(ShortcutInstallStatus.Unavailable, null, "Windows Desktop directory is unavailable.");
        }

        var shortcutPath = Path.Combine(desktopDirectory, "Abraxius Lattice.lnk");
        var iconPath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Abraxius", "Lattice", "lattice.ico");
        IconFactory.WriteIco(iconPath);
        var launch = ResolveLaunchSpec();

        CreateWindowsShortcut(shortcutPath, launch, iconPath);
        return new(ShortcutInstallStatus.Installed, shortcutPath, "Windows desktop shortcut installed.");
    }

    [SupportedOSPlatform("macos")]
    private static ShortcutInstallResult InstallMacOs()
    {
        var desktopDirectory = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
        if (string.IsNullOrWhiteSpace(desktopDirectory))
        {
            return new(ShortcutInstallStatus.Unavailable, null, "macOS Desktop directory is unavailable.");
        }

        var appPath = Path.Combine(desktopDirectory, "Abraxius Lattice.app");
        var contentsPath = Path.Combine(appPath, "Contents");
        var macOsPath = Path.Combine(contentsPath, "MacOS");
        var resourcesPath = Path.Combine(contentsPath, "Resources");
        Directory.CreateDirectory(macOsPath);
        Directory.CreateDirectory(resourcesPath);
        InstallPngIcon(
            Path.Combine(AppContext.BaseDirectory, "Assets", "Lattice.png"),
            Path.Combine(resourcesPath, "Lattice.png"));

        var launch = ResolveLaunchSpec();
        var infoPlist = """
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.abraxius.lattice</string>
<key>CFBundleName</key><string>Abraxius Lattice</string>
<key>CFBundleExecutable</key><string>LatticeLauncher</string>
<key>CFBundleIconFile</key><string>Lattice.png</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
""";
        WriteTextAtomically(Path.Combine(contentsPath, "Info.plist"), infoPlist);

        var launcher = "#!/bin/sh\nexec " + QuoteShell(launch.FileName);
        if (!string.IsNullOrWhiteSpace(launch.Arguments))
        {
            launcher += " " + launch.Arguments;
        }

        launcher += " \"$@\"\n";
        var launcherPath = Path.Combine(macOsPath, "LatticeLauncher");
        WriteTextAtomically(launcherPath, launcher);
        File.SetUnixFileMode(launcherPath, UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute | UnixFileMode.GroupRead | UnixFileMode.GroupExecute | UnixFileMode.OtherRead | UnixFileMode.OtherExecute);

        return new(ShortcutInstallStatus.Installed, appPath, "macOS application launcher installed.");
    }

    private static void InstallPngIcon(string source, string destination)
    {
        if (!File.Exists(source))
        {
            throw new FileNotFoundException("The Lattice artwork is not present in the application assets.", source);
        }

        Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
        var temporaryPath = destination + ".tmp";
        File.Copy(source, temporaryPath, overwrite: true);
        File.Move(temporaryPath, destination, overwrite: true);
    }

    private static string InstallLinuxIcon(string source, string iconDirectory)
    {
        if (!File.Exists(source))
        {
            throw new FileNotFoundException("The Lattice artwork is not present in the application assets.", source);
        }

        var png = File.ReadAllBytes(source);
        var hash = Convert.ToHexString(SHA256.HashData(png)).ToLowerInvariant()[..16];
        var destination = Path.Combine(iconDirectory, $"abraxius-lattice-{hash}.png");
        Directory.CreateDirectory(iconDirectory);
        var temporaryPath = destination + ".tmp";
        File.WriteAllBytes(temporaryPath, png);
        File.Move(temporaryPath, destination, overwrite: true);

        // Icon themes cache by filename. Remove only files generated by this
        // installer so a changed logo cannot remain hidden behind an old cache.
        foreach (var staleIcon in Directory.EnumerateFiles(iconDirectory, "abraxius-lattice-*.png"))
        {
            if (!string.Equals(staleIcon, destination, StringComparison.Ordinal))
            {
                File.Delete(staleIcon);
            }
        }

        var legacyIcon = Path.Combine(iconDirectory, "abraxius-lattice.png");
        if (File.Exists(legacyIcon))
        {
            File.Delete(legacyIcon);
        }

        return destination;
    }

    [SupportedOSPlatform("linux")]
    private static void RefreshLinuxDesktopCaches(string dataHome, string applicationsDirectory, string iconDirectory)
    {
        RunBestEffort("update-desktop-database", applicationsDirectory);
        var iconThemeDirectory = Path.GetDirectoryName(Path.GetDirectoryName(iconDirectory))
            ?? Path.Combine(dataHome, "icons", "hicolor");
        RunBestEffort("gtk-update-icon-cache", "-f", "-t", iconThemeDirectory);
    }

    private static void RunBestEffort(string executable, params string[] arguments)
    {
        try
        {
            using var process = new System.Diagnostics.Process
            {
                StartInfo = new System.Diagnostics.ProcessStartInfo
                {
                    FileName = executable,
                    UseShellExecute = false,
                    CreateNoWindow = true,
                },
            };
            foreach (var argument in arguments)
            {
                process.StartInfo.ArgumentList.Add(argument);
            }

            if (process.Start())
            {
                process.WaitForExit(2_000);
            }
        }
        catch (System.ComponentModel.Win32Exception)
        {
            // Desktop database helpers are optional; the files remain valid
            // even on minimal Linux installations without these utilities.
        }
        catch (InvalidOperationException)
        {
            // Refresh is best effort and must never block application startup.
        }
    }

    private static string BuildDesktopEntry(string iconPath)
    {
        var launch = ResolveLaunchSpec();
        var builder = new StringBuilder();
        builder.AppendLine("[Desktop Entry]");
        builder.AppendLine("Type=Application");
        builder.AppendLine("Name=Abraxius Lattice");
        builder.AppendLine("Comment=Native Intelligence Layer for Roblox");
        builder.AppendLine("Terminal=false");
        builder.AppendLine("Categories=Development;Utility;");
        builder.AppendLine("StartupWMClass=Lattice");
        builder.AppendLine("Icon=" + iconPath);
        builder.Append("Exec=").Append(QuoteDesktopArg(launch.FileName));
        if (!string.IsNullOrWhiteSpace(launch.Arguments))
        {
            builder.Append(' ').Append(launch.Arguments);
        }

        builder.AppendLine();
        return builder.ToString();
    }

    private static string? ResolveDesktopDirectory()
    {
        var explicitDirectory = Environment.GetEnvironmentVariable("XDG_DESKTOP_DIR");
        if (!string.IsNullOrWhiteSpace(explicitDirectory) && Path.IsPathRooted(explicitDirectory))
        {
            return Directory.Exists(explicitDirectory) ? explicitDirectory : null;
        }

        var knownDirectory = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
        return Directory.Exists(knownDirectory) ? knownDirectory : null;
    }

    private static (string FileName, string Arguments) ResolveLaunchSpec()
    {
        var processPath = Environment.ProcessPath ?? throw new InvalidOperationException("The process path is unavailable.");
        if (string.Equals(
                Path.GetFileNameWithoutExtension(processPath),
                "dotnet",
                StringComparison.OrdinalIgnoreCase))
        {
            var assemblyPath = Assembly.GetEntryAssembly()?.Location;
            if (!string.IsNullOrWhiteSpace(assemblyPath) && File.Exists(assemblyPath))
            {
                // Keep the host explicit because development machines may
                // have only a newer runtime installed. RollForward in the
                // runtimeconfig is authoritative; this flag also covers
                // older already-built output while it is being refreshed.
                return (processPath, "--roll-forward Major " + QuoteProcessArg(assemblyPath));
            }
        }

        return (processPath, string.Empty);
    }

    private static string QuoteDesktopArg(string value) =>
        "\"" + value.Replace("\\", "\\\\", StringComparison.Ordinal).Replace("\"", "\\\"", StringComparison.Ordinal).Replace("$", "\\$", StringComparison.Ordinal).Replace("`", "\\`", StringComparison.Ordinal) + "\"";

    private static string QuoteProcessArg(string value) =>
        "\"" + value.Replace("\"", "\\\"", StringComparison.Ordinal) + "\"";

    private static string QuoteShell(string value) =>
        "\"" + value.Replace("\\", "\\\\", StringComparison.Ordinal).Replace("\"", "\\\"", StringComparison.Ordinal) + "\"";

    private static void WriteTextAtomically(string path, string contents)
    {
        var temporaryPath = path + ".tmp";
        File.WriteAllText(temporaryPath, contents, new UTF8Encoding(encoderShouldEmitUTF8Identifier: false));
        File.Move(temporaryPath, path, overwrite: true);
    }

    [SupportedOSPlatform("linux")]
    [SupportedOSPlatform("macos")]
    private static void MarkExecutable(string path) =>
        File.SetUnixFileMode(path, UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute | UnixFileMode.GroupRead | UnixFileMode.GroupExecute | UnixFileMode.OtherRead | UnixFileMode.OtherExecute);

    private sealed record LaunchSpec(string FileName, string Arguments);

    [SupportedOSPlatform("windows")]
    private static void CreateWindowsShortcut(string path, (string FileName, string Arguments) launch, string iconPath)
    {
        var shellLinkType = Type.GetTypeFromCLSID(
            new Guid("00021401-0000-0000-C000-000000000046"), throwOnError: true)
            ?? throw new InvalidOperationException("Windows ShellLink COM class could not be resolved.");
        var shellLinkObject = Activator.CreateInstance(shellLinkType)
            ?? throw new InvalidOperationException("Windows ShellLink COM class could not be created.");
        var shellLink = (IShellLinkW)shellLinkObject;
        try
        {
            ThrowIfFailed(shellLink.SetPath(launch.FileName));
            ThrowIfFailed(shellLink.SetArguments(launch.Arguments));
            ThrowIfFailed(shellLink.SetWorkingDirectory(AppContext.BaseDirectory));
            ThrowIfFailed(shellLink.SetDescription("Abraxius Lattice"));
            ThrowIfFailed(shellLink.SetIconLocation(iconPath, 0));

            var persist = (IPersistFile)shellLink;
            ThrowIfFailed(persist.Save(path, true));
        }
        finally
        {
            Marshal.FinalReleaseComObject(shellLinkObject);
        }
    }

    private static void ThrowIfFailed(int result)
    {
        if (result < 0)
        {
            Marshal.ThrowExceptionForHR(result);
        }
    }

    [ComImport]
    [Guid("000214F9-0000-0000-C000-000000000046")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IShellLinkW
    {
        int GetPath([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder file, int maxPath, IntPtr findData, uint flags);
        int GetIDList(out IntPtr itemIdList);
        int SetIDList(IntPtr itemIdList);
        int GetDescription([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder name, int maxName);
        int SetDescription([MarshalAs(UnmanagedType.LPWStr)] string name);
        int GetWorkingDirectory([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder directory, int maxPath);
        int SetWorkingDirectory([MarshalAs(UnmanagedType.LPWStr)] string directory);
        int GetArguments([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder arguments, int maxPath);
        int SetArguments([MarshalAs(UnmanagedType.LPWStr)] string arguments);
        int GetHotkey(out short hotkey);
        int SetHotkey(short hotkey);
        int GetShowCmd(out int showCommand);
        int SetShowCmd(int showCommand);
        int GetIconLocation([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder iconPath, int maxPath, out int iconIndex);
        int SetIconLocation([MarshalAs(UnmanagedType.LPWStr)] string iconPath, int iconIndex);
        int SetRelativePath([MarshalAs(UnmanagedType.LPWStr)] string pathRel, uint reserved);
        int Resolve(IntPtr windowHandle, uint flags);
        int SetPath([MarshalAs(UnmanagedType.LPWStr)] string file);
    }

    [ComImport]
    [Guid("0000010B-0000-0000-C000-000000000046")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IPersistFile
    {
        int GetClassID(out Guid classId);
        int IsDirty();
        int Load([MarshalAs(UnmanagedType.LPWStr)] string fileName, uint mode);
        int Save([MarshalAs(UnmanagedType.LPWStr)] string fileName, [MarshalAs(UnmanagedType.Bool)] bool remember);
        int SaveCompleted([MarshalAs(UnmanagedType.LPWStr)] string fileName);
        int GetCurFile([MarshalAs(UnmanagedType.LPWStr)] out string fileName);
    }
}
