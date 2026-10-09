// Source only. C# 5 / .NET Framework 4. No process dispatch or runspace callbacks.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Security.Cryptography;
using System.Threading;

namespace Morrow.C28.Autonomous
{
    public sealed class CaptureFrame
    {
        public string Stream;
        public long Offset;
        public byte[] Bytes;
    }
    public sealed class StreamSnapshot
    {
        public long Observed, Emitted, SavedAck, Retained, Discarded;
        public bool EOF, DrainOnly, Unknown;
    }
    public sealed class PumpSnapshot
    {
        public StreamSnapshot Out, Err, Journal;
        public CaptureFrame[] Frames;
        public bool Terminal, ReadersEOF, JournalCaughtUp, ReadersJoined, CaptureFailed, Cancelled;
        public int? ActualExitCode;
        public long? TerminalJournalLength;
        public string TerminalJournalSHA256;
        public string[] Faults;
    }
    public sealed class CapturePump
    {
        private sealed class Lane
        {
            internal string Name;
            internal long Limit, Capacity, Observed, Emitted, Saved, Retained, Discarded;
            internal bool EOF, DrainOnly, Unknown;
            internal readonly List<CaptureFrame> Frames = new List<CaptureFrame>();
            internal readonly HashSet<long> EmittedEnds = new HashSet<long>();
            internal Lane(string name, long limit, long capacity)
            { Name = name; Limit = limit; Capacity = capacity; }
        }
        private readonly object gate = new object();
        private readonly ManualResetEvent handoff = new ManualResetEvent(false);
        private readonly string journalPath;
        private readonly Lane output = new Lane("Out", 128 * 1024, 32 * 1024);
        private readonly Lane error = new Lane("Err", 128 * 1024, 32 * 1024);
        private readonly Lane journal = new Lane("journal", 32L * 1024 * 1024, 16 * 1024);
        private readonly List<string> faults = new List<string>();
        private Thread outThread, errThread, worker;
        private byte[] outBuffer, errBuffer, journalBuffer;
        private SHA256 journalReadHash;
        private Process process;
        private Stream outPipe, errPipe;
        private FileStream journalFile, journalSeal;
        private bool prepared, prepareComplete, attachAttempted, cancelled, terminal;
        private bool captureFailed, journalCaughtUp, readersJoined, journalBroken;
        private int? exitCode;
        private long? frozenLength;
        private string frozenHash;

        public CapturePump(string journalPath)
        {
            if (journalPath == null || !Path.IsPathRooted(journalPath))
                throw new ArgumentException("Fixed absolute owned journal path required");
            this.journalPath = Path.GetFullPath(journalPath);
        }
        // These properties emit no frames and acknowledge no bytes.
        public bool CaptureFailed { get { lock (gate) { return captureFailed; } } }
        public bool Terminal { get { lock (gate) { return terminal; } } }

        private void Fault(string label, Lane lane)
        {
            captureFailed = true;
            if (lane != null) { lane.Unknown = true; lane.DrainOnly = true; }
            try { if (!faults.Contains(label)) faults.Add(label); } catch { }
        }
        public void Prepare()
        {
            lock (gate)
            {
                if (prepared) throw new InvalidOperationException("Prepare once");
                prepared = true;
            }
            try
            {
                lock (gate)
                {
                    // No scratch allocation after a returned child needs draining.
                    outBuffer = new byte[8192]; errBuffer = new byte[8192]; journalBuffer = new byte[8192];
                    outThread = new Thread(delegate() { ReadPipe(output, true); });
                    errThread = new Thread(delegate() { ReadPipe(error, false); });
                    worker = new Thread(JournalAndTerminal);
                    outThread.Name = "C28-Out"; errThread.Name = "C28-Err"; worker.Name = "C28-Journal-Terminal";
                    outThread.IsBackground = false; errThread.IsBackground = false; worker.IsBackground = false;
                }
                outThread.Start(); errThread.Start(); worker.Start();
                lock (gate) { prepareComplete = true; }
            }
            catch
            {
                lock (gate) { cancelled = true; Fault("PREPARE_THREAD_START_FAILED", null); }
                handoff.Set();
                JoinStartedForever(outThread); JoinStartedForever(errThread); JoinStartedForever(worker);
                throw;
            }
        }
        private static void JoinStartedForever(Thread thread)
        {
            while (thread != null && (thread.ThreadState & System.Threading.ThreadState.Unstarted) == 0)
            {
                try { thread.Join(); return; }
                catch (ThreadInterruptedException) { }
            }
        }
        public void Attach(Process retainedChild)
        {
            lock (gate)
            {
                if (!prepareComplete || cancelled || attachAttempted || retainedChild == null)
                { Fault("ATTACH_REJECTED", null); throw new InvalidOperationException("One prepared returned child only"); }
                attachAttempted = true;
                process = retainedChild; // Retain the exact returned object before risky stream getters.
            }
            try
            {
                Stream stdout = retainedChild.StandardOutput.BaseStream;
                lock (gate) { outPipe = stdout; }
                Stream stderr = retainedChild.StandardError.BaseStream;
                lock (gate) { errPipe = stderr; }
            }
            catch
            {
                lock (gate) { Fault("ATTACH_STREAM_HANDOFF_FAILED", output); Fault("ATTACH_STREAM_HANDOFF_FAILED", error); }
                throw;
            }
            finally { handoff.Set(); }
        }
        public void CancelBeforeAttach()
        {
            lock (gate)
            {
                if (!prepared || attachAttempted || process != null)
                { Fault("CANCEL_AFTER_ATTACH_REJECTED", null); throw new InvalidOperationException("No attached cancellation"); }
                cancelled = true;
            }
            handoff.Set();
        }
        private void Append(Lane lane, byte[] bytes, int count)
        {
            long offset = lane.Observed;
            if (offset > Int64.MaxValue - count) { Fault(lane.Name + "_COUNTER_OVERFLOW", lane); return; }
            lane.Observed += count;
            if (lane.Observed > lane.Limit) Fault(lane.Name + "_TOTAL_LIMIT", lane);
            if (!lane.DrainOnly && lane.Retained + count > lane.Capacity) Fault(lane.Name + "_RETENTION_LIMIT", lane);
            if (lane.DrainOnly) { lane.Discarded += count; return; }
            byte[] copy = new byte[count]; Buffer.BlockCopy(bytes, 0, copy, 0, count);
            lane.Frames.Add(new CaptureFrame { Stream = lane.Name, Offset = offset, Bytes = copy });
            lane.Retained += count;
        }
        private void ReadPipe(Lane lane, bool stdout)
        {
            handoff.WaitOne();
            lock (gate) { if (cancelled && process == null) return; }
            Stream pipe;
            lock (gate) { pipe = stdout ? outPipe : errPipe; }
            if (pipe == null)
            {
                lock (gate) { Fault(lane.Name + "_NO_RETURNED_STREAM", lane); }
                RetainUnreadable(); return;
            }
            byte[] bytes = stdout ? outBuffer : errBuffer;
            while (true)
            {
                int count;
                try { count = pipe.Read(bytes, 0, bytes.Length); }
                catch
                {
                    lock (gate) { Fault(lane.Name + "_READ_FAILED_EOF_UNOBSERVED", lane); }
                    // No second Read or fabricated EOF after an uncertain failed read.
                    RetainUnreadable(); return;
                }
                lock (gate)
                {
                    if (count == 0) { lane.EOF = true; return; }
                    long before = lane.Observed;
                    try { Append(lane, bytes, count); }
                    catch
                    {
                        Fault(stdout ? "Out_RETAIN_FAILED" : "Err_RETAIN_FAILED", lane);
                        if (lane.Observed == before) lane.Observed += count;
                        lane.Discarded += lane.Observed - before;
                    }
                }
            }
        }
        private static void RetainUnreadable()
        {
            // Deliberately keep the same unreadable resource/thread reachable.
            while (true) { try { Thread.Sleep(1000); } catch (ThreadInterruptedException) { } }
        }
        private void ObserveTerminal()
        {
            Process same;
            lock (gate) { if (terminal) return; same = process; }
            try
            {
                if (!same.HasExited) return;
                same.WaitForExit();
                int code = same.ExitCode;
                lock (gate) { exitCode = code; terminal = true; }
                // Actual exit is frozen BEFORE any journal length/hash validation.
            }
            catch { lock (gate) { Fault("SAME_PROCESS_TERMINAL_UNAVAILABLE", null); } }
        }
        private void CheckFrozenLength()
        {
            if (!frozenLength.HasValue || journalFile == null) return;
            try
            {
                if (journalFile.Length != frozenLength.Value)
                { journalCaughtUp = false; Fault("FROZEN_JOURNAL_LENGTH_CHANGED", journal); }
            }
            catch { journalCaughtUp = false; Fault("FROZEN_JOURNAL_LENGTH_UNAVAILABLE", journal); }
        }
        private void JournalAndTerminal()
        {
            handoff.WaitOne();
            lock (gate) { if (cancelled && process == null) return; }
            byte[] bytes = journalBuffer;
            try { journalReadHash = SHA256.Create(); }
            catch { lock (gate) { journalBroken = true; Fault("JOURNAL_HASH_INITIALIZATION_FAILED", journal); } }
            while (true)
            {
                ObserveTerminal();
                bool ended;
                lock (gate) { ended = terminal; }
                if (journalFile == null && !journalBroken)
                {
                    try
                    {
                        if (File.Exists(journalPath))
                        {
                            FileStream opened = new FileStream(journalPath, FileMode.Open, FileAccess.Read, FileShare.ReadWrite);
                            lock (gate) { journalFile = opened; }
                        }
                        else if (ended) { lock (gate) { journalBroken = true; Fault("TERMINAL_JOURNAL_ABSENT", journal); } }
                    }
                    catch { lock (gate) { journalBroken = true; Fault("JOURNAL_OPEN_FAILED", journal); } }
                }
                if (journalFile != null && !journalBroken)
                {
                    try
                    {
                        long length = journalFile.Length;
                        lock (gate)
                        {
                            if (ended && !frozenLength.HasValue) frozenLength = length;
                            if (length > journal.Limit)
                            { journalBroken = true; Fault("JOURNAL_FILE_LENGTH_LIMIT_NOT_EOF", journal); }
                            CheckFrozenLength();
                            if (length < journal.Observed)
                            { journalBroken = true; Fault("JOURNAL_TRUNCATED_NOT_EOF", journal); }
                        }
                        if (!journalBroken)
                        {
                            int requested;
                            lock (gate)
                            {
                                long available = length - journal.Observed;
                                // Never read an active empty file based on a stale zero
                                // length, or read beyond this already checked length.
                                requested = available > 0 ? (int)Math.Min(bytes.Length, available) : (ended ? bytes.Length : 0);
                            }
                            // -1 means no read was attempted, not an EOF observation.
                            int count = requested == 0 ? -1 : journalFile.Read(bytes, 0, requested);
                            if (count > 0)
                            {
                                lock (gate)
                                {
                                    if (journal.Observed + count <= journal.Limit)
                                        journalReadHash.TransformBlock(bytes, 0, count, bytes, 0);
                                    long before = journal.Observed;
                                    try { Append(journal, bytes, count); }
                                    catch
                                    {
                                        Fault("JOURNAL_RETAIN_FAILED", journal);
                                        if (journal.Observed == before) journal.Observed += count;
                                        journal.Discarded += journal.Observed - before;
                                    }
                                }
                            }
                            else if (ended)
                            {
                                lock (gate)
                                {
                                    CheckFrozenLength();
                                    if (journal.Observed != frozenLength.Value)
                                    { journalBroken = true; Fault("JOURNAL_END_NOT_FROZEN_BOUNDARY", journal); }
                                    else journal.EOF = true; // Only the actual zero Read above.
                                }
                                if (!journalBroken) SealAndValidateJournal();
                                return;
                            }
                        }
                    }
                    catch { lock (gate) { journalBroken = true; Fault("JOURNAL_READ_OR_VALIDATION_FAILED", journal); } }
                }
                if (ended && journalBroken) return; // Join is distinct from true EOF/capture acceptance.
                Thread.Sleep(5);
            }
        }
        private void SealAndValidateJournal()
        {
            // Keep this read-only, non-delete-sharing handle reachable. On Windows
            // it rejects later writers; Snapshot also checks frozen length each time.
            FileStream seal = new FileStream(journalPath, FileMode.Open, FileAccess.Read, FileShare.Read);
            lock (gate) { journalSeal = seal; }
            long length;
            lock (gate) { length = frozenLength.Value; }
            if (length > journal.Limit) { lock (gate) { Fault("JOURNAL_HASH_LIMIT", journal); } return; }
            if (seal.Length != length) { lock (gate) { Fault("JOURNAL_SEAL_LENGTH_CHANGED", journal); } return; }
            byte[] digest;
            using (SHA256 sha = SHA256.Create()) { digest = sha.ComputeHash(seal); }
            journalReadHash.TransformFinalBlock(new byte[0], 0, 0);
            byte[] observedDigest = journalReadHash.Hash;
            lock (gate)
            {
                CheckFrozenLength();
                if (seal.Length != length) { Fault("JOURNAL_POST_HASH_LENGTH_CHANGED", journal); return; }
                if (BitConverter.ToString(digest) != BitConverter.ToString(observedDigest))
                { journalCaughtUp = false; Fault("JOURNAL_CAPTURE_CONTENT_CHANGED", journal); return; }
                frozenHash = BitConverter.ToString(digest).Replace("-", "").ToLowerInvariant();
                journalCaughtUp = journal.EOF && !journalBroken && journal.Observed == length;
            }
        }
        private bool ValidAck(Lane lane, long ack)
        { return ack == lane.Saved || (ack > lane.Saved && ack <= lane.Emitted && lane.EmittedEnds.Contains(ack)); }
        private void ApplyAck(Lane lane, long ack)
        {
            lane.Saved = ack;
            while (lane.Frames.Count != 0)
            {
                CaptureFrame frame = lane.Frames[0];
                if (frame.Offset + frame.Bytes.Length > ack) break;
                lane.Retained -= frame.Bytes.Length; lane.Frames.RemoveAt(0);
            }
            lane.EmittedEnds.RemoveWhere(delegate(long end) { return end <= ack; });
        }
        private static StreamSnapshot CopyLane(Lane lane)
        {
            return new StreamSnapshot { Observed = lane.Observed, Emitted = lane.Emitted,
                SavedAck = lane.Saved, Retained = lane.Retained, Discarded = lane.Discarded,
                EOF = lane.EOF, DrainOnly = lane.DrainOnly, Unknown = lane.Unknown };
        }
        private void HostDrain(Lane lane, string label)
        {
            Fault(label, lane);
            // The host may already have observed an unsaved prefix. Do not resend
            // those offsets as new bytes, and never manufacture its durable ACK.
            lane.Discarded += lane.Retained;
            lane.Retained = 0;
            lane.Frames.Clear(); lane.EmittedEnds.Clear();
        }
        public PumpSnapshot Snapshot(long ackOut, long ackErr, long ackJournal, int drainMask)
        {
            lock (gate)
            {
                CheckFrozenLength();
                if (drainMask < 0 || drainMask > 7) Fault("DRAIN_MASK_INVALID", null);
                if (drainMask < 0 || drainMask > 7 || !ValidAck(output, ackOut) || !ValidAck(error, ackErr) || !ValidAck(journal, ackJournal))
                    Fault("SAVED_ACK_NOT_PRIOR_EMITTED_BOUNDARY", null);
                else { ApplyAck(output, ackOut); ApplyAck(error, ackErr); ApplyAck(journal, ackJournal); }
                if (drainMask >= 0 && drainMask <= 7)
                {
                    if ((drainMask & 1) != 0) HostDrain(output, "HOST_OUT_SAVE_FAILED");
                    if ((drainMask & 2) != 0) HostDrain(error, "HOST_ERR_SAVE_FAILED");
                    if ((drainMask & 4) != 0) HostDrain(journal, "HOST_JOURNAL_SAVE_FAILED");
                }
                List<CaptureFrame> frames = new List<CaptureFrame>();
                foreach (Lane lane in new Lane[] { output, error, journal })
                {
                    foreach (CaptureFrame frame in lane.Frames)
                    {
                        frames.Add(new CaptureFrame { Stream = frame.Stream, Offset = frame.Offset, Bytes = (byte[])frame.Bytes.Clone() });
                        long end = frame.Offset + frame.Bytes.Length;
                        if (end > lane.Emitted) lane.Emitted = end;
                        lane.EmittedEnds.Add(end);
                    }
                }
                return new PumpSnapshot { Out = CopyLane(output), Err = CopyLane(error), Journal = CopyLane(journal),
                    Frames = frames.ToArray(), Terminal = terminal, ActualExitCode = exitCode,
                    TerminalJournalLength = frozenLength, TerminalJournalSHA256 = frozenHash,
                    ReadersEOF = output.EOF && error.EOF, JournalCaughtUp = journalCaughtUp,
                    ReadersJoined = readersJoined, CaptureFailed = captureFailed, Cancelled = cancelled,
                    Faults = faults.ToArray() };
            }
        }
        public bool WaitReaders(int timeoutMilliseconds)
        {
            if (timeoutMilliseconds < 0) throw new ArgumentOutOfRangeException("timeoutMilliseconds");
            Thread[] threads;
            lock (gate)
            {
                if (!prepared) return false;
                threads = new Thread[] { outThread, errThread, worker };
            }
            Stopwatch time = Stopwatch.StartNew();
            foreach (Thread thread in threads)
            {
                if (thread == null) return false;
                if ((thread.ThreadState & System.Threading.ThreadState.Unstarted) != 0) return false;
                int remaining = Math.Max(0, timeoutMilliseconds - (int)Math.Min(Int32.MaxValue, time.ElapsedMilliseconds));
                if (!thread.Join(remaining)) return false;
            }
            lock (gate) { readersJoined = true; }
            return true; // Only thread joining; never guest/Workbench join or acceptance.
        }
    }
}
