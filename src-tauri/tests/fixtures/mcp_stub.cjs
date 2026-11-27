const readline = require('node:readline')

const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity })
const send = (message) => process.stdout.write(`${JSON.stringify(message)}\n`)

rl.on('line', (line) => {
  let message
  try {
    message = JSON.parse(line)
  } catch {
    return
  }
  if (message.id === undefined || message.id === null) return // notifications/initialized
  const reply = (result) => send({ jsonrpc: '2.0', id: message.id, result })
  if (message.method === 'initialize') {
    reply({
      protocolVersion: message.params?.protocolVersion || '2026-07-28',
      capabilities: { tools: {} },
      serverInfo: { name: 'stub', version: '1.0.0' },
    })
    return
  }
  if (message.method === 'tools/list') {
    reply({
      tools: [
        {
          name: 'echo',
          description: 'Echo the text argument',
          inputSchema: {
            type: 'object',
            properties: { text: { type: 'string' } },
            required: ['text'],
          },
        },
      ],
    })
    return
  }
  if (message.method === 'tools/call') {
    const text = message.params?.arguments?.text || ''
    reply({ content: [{ type: 'text', text: `stub: ${text}` }], isError: false })
    return
  }
  if (message.method === 'ping') {
    reply({})
    return
  }
  send({
    jsonrpc: '2.0',
    id: message.id,
    error: { code: -32601, message: `unknown method ${message.method}` },
  })
})
