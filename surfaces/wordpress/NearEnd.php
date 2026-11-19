<?php
/**
 * A small client of the Conduit near end over a Unix socket.
 *
 * The near end is a separate process: it holds the socket to the broker
 * and does the sealing.  This file speaks to it over a Unix socket, in
 * plain JSON, one message at a time:
 *
 *     plugin  ->  {"t":"publish","s":"dev.conduit.wordpress.post/v1","c":"posts","p":{...}}
 *     near end ->  {"t":"publish","s":"...","c":"...","seq":12,"p":{...}}
 *
 * A message on that socket is a four-byte big-endian length, and then
 * that many bytes of JSON.
 *
 * What this file publishes is plaintext on that socket.  What leaves the
 * machine is not: the near end seals it under the key of the tunnel, and
 * the broker never sees it open.  This file holds no key and opens no
 * socket to the broker.
 *
 * There is no second vocabulary: the messages on that socket are the
 * frames of the tunnel.  And this file does not wait for the tunnel to
 * be up -- the near end holds what is published until there is a key to
 * seal it under, and sends it then.
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
     * The channel this plugin publishes on.
     *
     * @var string
     */
    private $channel;

    /**
     * The stream of the socket, or null when it is not open.
     *
     * @var resource|null
     */
    private $stream = null;

    /**
     * Whether the tunnel is up.
     *
     * The near end says so with the welcome frame.  Nothing waits for
     * it: the near end holds what is published until there is a key to
     * seal it under, and sends it then.
     *
     * @var bool
     */
    private $up = false;

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
     * @param string      $channel     the channel this plugin publishes on.
     */
    public function __construct($socket_path = null, $channel = 'posts')
    {
        if ($socket_path === null || $socket_path === '') {
            $socket_path = self::default_socket_path();
        }
        $this->socket_path = $socket_path;
        $this->channel = (string) $channel;
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
     * Whether the tunnel is up.
     *
     * @return bool
     */
    public function up()
    {
        return $this->up;
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
     * The frame is the frame of the tunnel, with a plaintext payload: the
     * near end seals it, and the broker never sees it open.  The publish
     * does not wait for the tunnel to be up; the near end holds it until
     * there is a key to seal it under.
     *
     * @param string $schema  The schema of the payload.
     * @param array  $payload The payload, as JSON.
     */
    public function publish($schema, array $payload)
    {
        $this->open();
        $this->send(array(
            't'   => 'publish',
            's'   => $schema,
            'c'   => $this->channel,
            'seq' => 0,
            'p'   => $payload,
        ));
    }

    /**
     * Say where the broker is.
     *
     * @param string $broker   The address of the broker.
     * @param string $secret   The secret of the grant.
     * @param string $instance The instance of the tunnel.
     */
    public function connect_to($broker, $secret, $instance)
    {
        $this->open();
        $this->send(array(
            't'   => 'connect',
            's'   => '',
            'c'   => $this->channel,
            'seq' => 0,
            'p'   => array(
                'broker'   => $broker,
                'secret'   => $secret,
                'instance' => $instance,
            ),
        ));
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
        if (isset($message['t']) && $message['t'] === 'welcome') {
            // The broker welcomed the near end: the tunnel is up.
            $this->up = true;
        }
        if (isset($message['t']) && $message['t'] === 'error') {
            $payload = isset($message['p']) && is_array($message['p']) ? $message['p'] : array();
            $this->last_error = isset($payload['message'])
                ? (string) $payload['message']
                : 'the near end refused the message';
        }
    }
}
