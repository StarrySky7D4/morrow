// Explicit synthetic entry only. The library itself never starts a process.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Text;
using System.Threading;
using Morrow.C28.Autonomous;

internal static class PumpTests
{
    private sealed class Case
    {
        internal string Name, Directory;
        internal CapturePump Pump;
        internal Process Child;
        internal bool StartAttempted, NoStart, AttachAttempted, NoSnapshotBeforeExit;
        internal long NoPollMilliseconds;
    }
    private static readonly List<Case> owners = new List<Case>();
    private static string root;
    private static Case small;
    private static void Need(bool condition, string message)
    { if (!condition) throw new InvalidOperationException(message); }
    private static string Quote(string value)
    { return "\"" + value.Replace("\\", "\\\\").Replace("\"", "\\\"").Replace("\r", "\\r").Replace("\n", "\\n") + "\""; }
    private static Case Start(string name, string mode)
    {
        string dir = Path.Combine(root, name);
        Need(!Directory.Exists(dir), "fresh case only"); Directory.CreateDirectory(dir);
        using (FileStream file = new FileStream(Path.Combine(dir, "journal.bin"), FileMode.CreateNew, FileAccess.Write)) { }
        Case item = new Case { Name = name, Directory = dir, Pump = new CapturePump(Path.Combine(dir, "journal.bin")), Child = new Process() };
        owners.Add(item); // Strong owner precedes Prepare/Start/Attach on every path.
        item.Pump.Prepare();
        item.Child.StartInfo = new ProcessStartInfo {
            FileName = Assembly.GetExecutingAssembly().Location,
            Arguments = "--child-burst \"" + dir + "\" " + mode,
            WorkingDirectory = dir, UseShellExecute = false,
            RedirectStandardOutput = true, RedirectStandardError = true,
            RedirectStandardInput = true, CreateNoWindow = true };
        item.StartAttempted = true;
        if (!item.Child.Start())
        {
            item.NoStart = true; item.Pump.CancelBeforeAttach();
            throw new InvalidOperationException("synthetic Start returned false");
        }
        item.AttachAttempted = true; item.Pump.Attach(item.Child);
        return item;
    }
    private static void Finish(Case item)
    {
        if (!item.StartAttempted || item.NoStart)
        {
            if (!item.NoStart) item.Pump.CancelBeforeAttach();
        }
        else if (!item.AttachAttempted)
        {
            item.AttachAttempted = true;
            try { item.Pump.Attach(item.Child); } catch { }
        }
        // No timeout abandonment, kill, respawn or replacement identity.
        while (item.StartAttempted && !item.NoStart)
        {
            try { if (item.Child.WaitForExit(100)) break; } catch { }
            Thread.Sleep(10);
        }
        while (true)
        {
            try { if (item.Pump.WaitReaders(100)) break; } catch { }
            Thread.Sleep(10);
        }
    }
    private static PumpSnapshot Done(Case item)
    {
        Finish(item);
        PumpSnapshot snap = item.Pump.Snapshot(0, 0, 0, 0);
        Need(snap.Terminal && snap.ActualExitCode == 0 && snap.ReadersEOF && snap.ReadersJoined, "real terminal and pipe EOF/join required");
        return snap;
    }
    private static string LaneJSON(StreamSnapshot lane)
    {
        return "{\"Observed\":" + lane.Observed + ",\"Emitted\":" + lane.Emitted + ",\"SavedAck\":" + lane.SavedAck +
            ",\"Retained\":" + lane.Retained + ",\"Discarded\":" + lane.Discarded + ",\"EOF\":" + Bool(lane.EOF) +
            ",\"DrainOnly\":" + Bool(lane.DrainOnly) + ",\"Unknown\":" + Bool(lane.Unknown) + "}";
    }
    private static string Bool(bool value) { return value ? "true" : "false"; }
    private static void SaveObservation(Case item, PumpSnapshot snap)
    {
        string json = "{\"schema\":\"C28_PUMP_REAL_SYNTHETIC_CASE_V1\",\"case\":" + Quote(item.Name) +
            ",\"returned_pid\":" + item.Child.Id + ",\"actual_exit\":" + (snap.ActualExitCode.HasValue ? snap.ActualExitCode.Value.ToString() : "null") +
            ",\"Terminal\":" + Bool(snap.Terminal) + ",\"ReadersEOF\":" + Bool(snap.ReadersEOF) + ",\"ReadersJoined\":" + Bool(snap.ReadersJoined) +
            ",\"JournalCaughtUp\":" + Bool(snap.JournalCaughtUp) + ",\"CaptureFailed\":" + Bool(snap.CaptureFailed) +
            ",\"TerminalJournalLength\":" + (snap.TerminalJournalLength.HasValue ? snap.TerminalJournalLength.Value.ToString() : "null") +
            ",\"no_snapshot_before_child_exit\":" + Bool(item.NoSnapshotBeforeExit) + ",\"no_poll_milliseconds\":" + item.NoPollMilliseconds +
            ",\"Out\":" + LaneJSON(snap.Out) + ",\"Err\":" + LaneJSON(snap.Err) + ",\"journal\":" + LaneJSON(snap.Journal) +
            ",\"SDK_qualified\":false,\"release_eligible\":false}";
        byte[] bytes = Encoding.UTF8.GetBytes(json + "\n"); Need(bytes.Length <= 16384, "case JSON bound");
        using (FileStream file = new FileStream(Path.Combine(item.Directory, "case-observation.json"), FileMode.CreateNew, FileAccess.Write))
        { file.Write(bytes, 0, bytes.Length); file.Flush(true); }
    }
    private static void PreparedCancelWithoutChild()
    {
        CapturePump pump = new CapturePump(Path.Combine(root, "unused-journal.bin"));
        pump.Prepare(); pump.CancelBeforeAttach();
        while (!pump.WaitReaders(100)) { }
        PumpSnapshot snap = pump.Snapshot(0, 0, 0, 0);
        Need(snap.Cancelled && snap.ReadersJoined && !snap.Terminal && !snap.ReadersEOF && !snap.Journal.EOF && !snap.CaptureFailed, "cancel is not child EOF");
    }
    private static void BurstWithoutSnapshotDrains()
    {
        Case item = Start("burst-no-poll", "burst");
        Stopwatch interval = Stopwatch.StartNew();
        // Absolutely no Snapshot while this child is emitting beyond pipe/queue capacity.
        while (!item.Child.WaitForExit(100)) { }
        item.NoPollMilliseconds = interval.ElapsedMilliseconds; item.NoSnapshotBeforeExit = true;
        PumpSnapshot snap = Done(item);
        Need(snap.Out.Observed == 655360 && snap.Err.Observed == 655360 && snap.Journal.Observed == 196608, "all real burst bytes drained");
        Need(snap.CaptureFailed && snap.Out.Unknown && snap.Err.Unknown && snap.Journal.Unknown, "capacity faults sticky");
        Need(snap.Out.Retained <= 32768 && snap.Err.Retained <= 32768 && snap.Journal.Retained <= 16384, "bounded retention");
        Need(snap.Out.SavedAck == 0 && snap.Err.SavedAck == 0 && snap.Journal.SavedAck == 0, "no invented ACK without host");
        Need(snap.Out.Discarded > 0 && snap.Err.Discarded > 0 && snap.Journal.Discarded > 0, "discard counted");
        SaveObservation(item, snap);
    }
    private static void CopiedFramesAndSavedAckBoundaries()
    {
        small = Start("small-ack", "small");
        PumpSnapshot snap = Done(small);
        Need(!snap.CaptureFailed && snap.JournalCaughtUp && snap.Journal.EOF, "small complete capture");
        Need(snap.Out.Observed == 10000 && snap.Err.Observed == 9000 && snap.Journal.Observed == 12000, "small counts");
        Need(snap.Out.SavedAck == 0 && snap.Err.SavedAck == 0 && snap.Journal.SavedAck == 0, "emitted is not saved");
        Need(snap.Frames.Length > 0, "frames available");
        byte prior = snap.Frames[0].Bytes[0]; snap.Frames[0].Bytes[0] ^= 0x7f;
        PumpSnapshot copied = small.Pump.Snapshot(0, 0, 0, 0);
        Need(copied.Frames[0].Bytes[0] == prior, "snapshot bytes are independent copies");
        PumpSnapshot acked = small.Pump.Snapshot(copied.Out.Emitted, copied.Err.Emitted, copied.Journal.Emitted, 0);
        Need(acked.Out.SavedAck == 10000 && acked.Err.SavedAck == 9000 && acked.Journal.SavedAck == 12000, "ACK only emitted full boundaries");
        Need(acked.Frames.Length == 0 && acked.Out.Retained == 0 && acked.Err.Retained == 0 && acked.Journal.Retained == 0 && !acked.CaptureFailed, "saved prefix releases only bounded buffers");
        SaveObservation(small, acked);
    }
    private static void InvalidAckIsAtomicAndSticky()
    {
        Case item = Start("invalid-ack", "small"); PumpSnapshot first = Done(item);
        PumpSnapshot snap = item.Pump.Snapshot(first.Out.Emitted + 1, first.Err.Emitted, first.Journal.Emitted, 0);
        Need(snap.CaptureFailed && snap.Out.SavedAck == 0 && snap.Err.SavedAck == 0 && snap.Journal.SavedAck == 0, "one over-emitted ACK changes no lane");
        long nonBoundary = -1;
        foreach (CaptureFrame frame in first.Frames)
            if (frame.Stream == "Out" && frame.Bytes.Length > 1) { nonBoundary = frame.Offset + 1; break; }
        Need(nonBoundary > 0, "fixed synthetic output supplies a multi-byte frame");
        snap = item.Pump.Snapshot(nonBoundary, 0, 0, 0);
        Need(snap.Out.SavedAck == 0, "inside-frame ACK is not a durable boundary");
        snap = item.Pump.Snapshot(first.Out.Emitted, first.Err.Emitted, first.Journal.Emitted, 0);
        Need(snap.Out.SavedAck == 10000 && snap.Err.SavedAck == 9000 && snap.Journal.SavedAck == 12000, "subsequent valid ACK is data only");
        snap = item.Pump.Snapshot(9999, 9000, 12000, 0);
        Need(snap.Out.SavedAck == 10000, "ACK cannot regress");
        snap = item.Pump.Snapshot(10000, 9000, 12000, 8);
        Need(Array.IndexOf(snap.Faults, "DRAIN_MASK_INVALID") >= 0 && snap.Out.SavedAck == 10000, "invalid mask has no ACK effect");
        Need(snap.CaptureFailed, "later EOF/valid ACK cannot erase Unknown");
        SaveObservation(item, snap);
    }
    private static void HostDrainDropsOnlyUnsavedLane()
    {
        Case item = Start("single-sink-drain", "small"); PumpSnapshot first = Done(item);
        PumpSnapshot drained = item.Pump.Snapshot(0, 0, 0, 1);
        Need(drained.Out.SavedAck == 0 && drained.Out.Retained == 0 && drained.Out.Discarded == 10000 && drained.Out.Emitted == first.Out.Emitted, "host drain is loss, not ACK");
        foreach (CaptureFrame frame in drained.Frames) Need(frame.Stream != "Out", "failed host sink gets no old offsets");
        PumpSnapshot saved = item.Pump.Snapshot(0, drained.Err.Emitted, drained.Journal.Emitted, 0);
        Need(saved.CaptureFailed && saved.Out.Unknown && !saved.Err.Unknown && !saved.Journal.Unknown && saved.Err.SavedAck == 9000 && saved.Journal.SavedAck == 12000, "healthy lanes retain independent progress");
        SaveObservation(item, saved);
    }
    private static void JournalTruncationNeverCreatesEOF()
    {
        Case item = Start("journal-truncate", "truncate");
        PumpSnapshot snap;
        Stopwatch budget = Stopwatch.StartNew();
        do
        {
            snap = item.Pump.Snapshot(0, 0, 0, 0);
            Need(budget.ElapsedMilliseconds < 4000, "finite synthetic handshake budget");
            Thread.Sleep(1);
        } while (snap.Journal.Observed != 12000);
        using (FileStream file = new FileStream(Path.Combine(item.Directory, "journal.bin"), FileMode.Open, FileAccess.Write, FileShare.ReadWrite)) { file.SetLength(4); file.Flush(true); }
        using (FileStream release = new FileStream(Path.Combine(item.Directory, "release"), FileMode.CreateNew, FileAccess.Write)) { }
        snap = Done(item);
        Need(snap.CaptureFailed && snap.Journal.Unknown && !snap.Journal.EOF && !snap.JournalCaughtUp, "truncation is not journal EOF");
        SaveObservation(item, snap);
    }
    private static void OversizeJournalRejectsWithoutReadingPayload()
    {
        Case item = Start("journal-oversize", "oversize"); PumpSnapshot snap = Done(item);
        Need(snap.CaptureFailed && snap.Journal.Unknown && !snap.Journal.EOF && !snap.JournalCaughtUp, "oversize is not EOF");
        Need(snap.Journal.Observed == 0 && snap.Journal.Retained == 0, "reject sparse oversize before payload read");
        Need(new FileInfo(Path.Combine(item.Directory, "journal.bin")).Length == 32L * 1024 * 1024 + 1, "fixed logical oversize");
        SaveObservation(item, snap);
    }
    private static int Child(string directory, string mode)
    {
        Need(Path.IsPathRooted(directory) && Directory.Exists(directory), "existing synthetic directory");
        Need(mode == "small" || mode == "burst" || mode == "truncate" || mode == "oversize", "fixed child scenario");
        string path = Path.Combine(directory, "journal.bin"); Need(File.Exists(path) && new FileInfo(path).Length == 0, "fresh empty journal");
        int outBytes = mode == "burst" ? 655360 : 10000, errBytes = mode == "burst" ? 655360 : 9000;
        int journalBytes = mode == "burst" ? 196608 : 12000;
        byte[] bytes = new byte[8192]; for (int i = 0; i < bytes.Length; i++) bytes[i] = (byte)'J';
        using (FileStream file = new FileStream(path, FileMode.Append, FileAccess.Write, FileShare.ReadWrite))
        {
            if (mode == "oversize") file.SetLength(32L * 1024 * 1024 + 1);
            else for (int left = journalBytes; left > 0; left -= Math.Min(left, bytes.Length)) file.Write(bytes, 0, Math.Min(left, bytes.Length));
            file.Flush(true);
        }
        if (mode == "truncate")
        {
            Stopwatch wait = Stopwatch.StartNew();
            while (!File.Exists(Path.Combine(directory, "release")) && wait.ElapsedMilliseconds < 5000) Thread.Sleep(1);
            if (!File.Exists(Path.Combine(directory, "release"))) return 2;
        }
        for (int i = 0; i < bytes.Length; i++) bytes[i] = (byte)'O';
        Stream stdout = Console.OpenStandardOutput();
        for (int left = outBytes; left > 0; left -= Math.Min(left, bytes.Length)) stdout.Write(bytes, 0, Math.Min(left, bytes.Length)); stdout.Flush();
        for (int i = 0; i < bytes.Length; i++) bytes[i] = (byte)'E';
        Stream stderr = Console.OpenStandardError();
        for (int left = errBytes; left > 0; left -= Math.Min(left, bytes.Length)) stderr.Write(bytes, 0, Math.Min(left, bytes.Length)); stderr.Flush();
        return 0;
    }
    private static void Test(string name, Action method)
    { method(); Console.WriteLine("TEST|" + name + "|PASS"); }
    private static int Main(string[] args)
    {
        if (args.Length == 3 && args[0] == "--child-burst") return Child(args[1], args[2]);
        if (args.Length != 2 || args[0] != "--run-synthetic") return 78;
        root = Path.GetFullPath(args[1]);
        Need(Path.IsPathRooted(args[1]) && !Directory.Exists(root) && !File.Exists(root), "fresh absolute synthetic run root");
        Directory.CreateDirectory(root);
        try
        {
            Test("PreparedCancelWithoutChild", PreparedCancelWithoutChild);
            Test("BurstWithoutSnapshotDrains", BurstWithoutSnapshotDrains);
            Test("CopiedFramesAndSavedAckBoundaries", CopiedFramesAndSavedAckBoundaries);
            Test("InvalidAckIsAtomicAndSticky", InvalidAckIsAtomicAndSticky);
            Test("HostDrainDropsOnlyUnsavedLane", HostDrainDropsOnlyUnsavedLane);
            Test("JournalTruncationNeverCreatesEOF", JournalTruncationNeverCreatesEOF);
            Test("OversizeJournalRejectsWithoutReadingPayload", OversizeJournalRejectsWithoutReadingPayload);
            Console.WriteLine("SUMMARY|7|PASS|LOCAL_SYNTHETIC_ONLY_SDK_FALSE");
            return 0;
        }
        catch (Exception error) { Console.Error.WriteLine("FAIL|" + error.GetType().Name + "|LOCAL_SYNTHETIC_ONLY"); return 1; }
        finally { foreach (Case item in owners) Finish(item); }
    }
}
