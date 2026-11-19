<?php
/**
 * A small client of the Conduit near end over a Unix socket.
 *
 * The near end is a separate process: it holds the socket to the broker
 * and does the sealing.  This file speaks to it over a Unix socket, in
 * plain JSON, one message at a time:
 *
 *     plugin  ->  {"op":"publish","schema":"dev.conduit.wordpress.post/v1","payload":{...}}
 *     near end ->  {"op":"status","surface":"...","agreed":true,"connected":true,"seq":12}
 *
 * A message on that socket is a four-byte big-endian length, and then
 * that many bytes of JSON.
 *
 * What this file publishes is plaintext on that socket.  What leaves the
 * machine is not: the near end seals it under the key of the tunnel, and
 * the broker never sees it open.  This file holds no key and opens no
 * socket to the broker.
 *
 * @package Conduit
 */

/**
 * The client of a near end over a Unix socket.
 */
class Conduit_NearEnd
{
    /**
     * The path of the socket of the near end.
     *
     * @var string
     */
    private $socket_path;

    /**
     * How long a publish waits for the tunnel to agree, in seconds.
     *
     * @var float
     */
    private $timeout;

    /**
     * The stream of the socket, or null when it is not open.
     *
     * @var resource|null
     */
    private $stream = null;

    /**
     * Whether the two ends have agreed their keys.
     *
     * The near end cannot seal anything until they have, so nothing is
     * published before this is true.
     *
     * @var bool
     */
    private $agreed = false;

    /**
     * The last thing the near end complained about.
     *
     * @var string|null
     */
    private $last_error = null;

    /**
     * A client of the near end.
     *
     * @param string|null $socket_path the path of the socket; the default when it is null.
     * @param float       $timeout     how long a publish waits for the tunnel to agree, in seconds.
     */
    public function __construct($socket_path = null, $timeout = 10.0)
    {
        if ($socket_path === null || $socket_path === '') {
            $socket_path = self::default_socket_path();
        }
        $this->socket_path = $socket_path;
        $this->timeout = (float) $timeout;
    }

    /**
     * The socket of the near end, from wherever the site put it.
     *
     * The environment variable first, then the option of the site, and then
     * the default.
     *
     * @return string
     */
    public static function default_socket_path()
    {
        $path = getenv('CONDUIT_SOCKET');
        if (is_string($path) && $path !== '') {
            return $path;
        }
        if (function_exists('get_option')) {
            $path = get_option('conduit_near_end_socket');
            if (is_string($path) && $path !== '') {
                return $path;
            }
        }
        return '/tmp/conduit-wordpress.sock';
    }

    /**
     * Whether the two ends have agreed their keys.
     *
     * @return bool
     */
    public function agreed()
    {
        return $this->agreed;
    }

    /**
     * The last thing the near end complained about.
     *
     * @return string|null
     */
    public function last_error()
    {
        return $this->last_error;
    }

    /**
     * Open the socket to the near end.
     *
     * Opening it twice is not a mistake: the plugin publishes more than
     * once, and the socket is opened once.
     *
     * @throws RuntimeException When the near end is not there.
     */
    public function open()
    {
        if ($this->stream !== null) {
            return;
        }

        $errno = 0;
        $errstr = '';
        $stream = @stream_socket_client('unix://' . $this->socket_path, $errno, $errstr, 5.0);
        if ($stream === false) {
            throw new RuntimeException(sprintf(
                'the near end did not answer at %s (%d: %s)',
                $this->socket_path,
                $errno,
                $errstr
            ));
        }
        stream_set_blocking($stream, true);
        stream_set_timeout($stream, 5);
        $this->stream = $stream;
    }

    /**
     * Close the socket.
     */
    public function close()
    {
        if ($this->stream !== null) {
            fclose($this->stream);
            $this->stream = null;
        }
    }

    /**
     * Publish what the app knows.
     *
     * The near end cannot seal anything until the two ends have agreed
     * their keys, and a publish before that is refused.  So the publish
     * waits for the tunnel to agree, with a timeout.
     *
     * @param string $schema  The schema of the payload.
     * @param array  $payload The payload, as JSON.
     *
     * @throws RuntimeException When the tunnel does not agree in time.
     */
    public function publish($schema, array $payload)
    {
        $this->open();
        if (!$this->wait_until_agreed()) {
            throw new RuntimeException('the near end did not agree the keys of the tunnel');
        }
        $this->send(array(
            'op'      => 'publish',
            'schema'  => $schema,
            'payload' => $payload,
        ));
    }

    /**
     * Ask how the tunnel is.
     */
    public function status()
    {
        $this->open();
        $this->send(array('op' => 'status'));
    }

    /**
     * Wait until the tunnel has a key.
     *
     * The two ends agree their keys when they have said hello to each
     * other, and that takes a moment.  The near end answers a status with
     * the state of the tunnel, so the question is asked again and again
     * until the answer is yes, or until the wait is over.
     *
     * @param float|null $timeout How long to wait, in seconds.
     *
     * @return bool Whether the tunnel agreed.
     */
    public function wait_until_agreed($timeout = null)
    {
        $wait = $timeout === null ? $this->timeout : (float) $timeout;
        $deadline = microtime(true) + $wait;
        while (microtime(true) < $deadline) {
            if ($this->agreed) {
                return true;
            }
            $this->status();
            $message = $this->receive();
            if (is_array($message)) {
                $this->note($message);
            }
            usleep(50000);
        }
        return $this->agreed;
    }

    /**
     * Say something to the near end: a length, then the bytes.
     *
     * @param array $message The message, as JSON.
     *
     * @throws RuntimeException When the message could not be written.
     */
    private function send(array $message)
    {
        if ($this->stream === null) {
            throw new RuntimeException('the socket to the near end is not open');
        }

        $body = json_encode($message, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
        if ($body === false) {
            throw new RuntimeException('the message could not be encoded: ' . json_last_error_msg());
        }

        $frame = pack('N', strlen($body)) . $body;
        $length = strlen($frame);
        $written = 0;
        while ($written < $length) {
            $chunk = fwrite($this->stream, substr($frame, $written));
            if ($chunk === false || $chunk === 0) {
                throw new RuntimeException('the message could not be written to the near end');
            }
            $written += $chunk;
        }
    }

    /**
     * Read one message from the near end.
     *
     * @return array|null The message, or null when nothing came in time.
     */
    private function receive()
    {
        if ($this->stream === null) {
            throw new RuntimeException('the socket to the near end is not open');
        }

        $header = $this->read_exactly(4);
        if ($header === null) {
            return null;
        }
        $parts = unpack('Nlength', $header);
        $length = (int) $parts['length'];
        if ($length > 1048576) {
            throw new RuntimeException('the near end sent a message that is too long');
        }

        $body = $length > 0 ? $this->read_exactly($length) : '';
        if ($body === null) {
            return null;
        }

        $message = json_decode($body, true);
        return is_array($message) ? $message : null;
    }

    /**
     * Read exactly that many bytes.
     *
     * @param int $length How many bytes.
     *
     * @return string|null The bytes, or null when they did not all come.
     */
    private function read_exactly($length)
    {
        $bytes = '';
        while (strlen($bytes) < $length) {
            $chunk = fread($this->stream, $length - strlen($bytes));
            if ($chunk === false || $chunk === '') {
                return null;
            }
            $bytes .= $chunk;
        }
        return $bytes;
    }

    /**
     * Remember what the near end said.
     *
     * @param array $message The message.
     */
    private function note(array $message)
    {
        if (isset($message['op']) && $message['op'] === 'status' && !empty($message['agreed'])) {
            $this->agreed = true;
        }
        if (isset($message['op']) && $message['op'] === 'error') {
            $this->last_error = isset($message['message'])
                ? (string) $message['message']
                : 'the near end refused the message';
        }
    }
}
