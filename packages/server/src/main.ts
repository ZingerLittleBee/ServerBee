import { NestFactory } from '@nestjs/core'
import { AppModule } from './app.module'
import * as cookieParser from 'cookie-parser'
import helmet from 'helmet'
import { NestExpressApplication } from '@nestjs/platform-express'
import { WsAdapter } from '@nestjs/platform-ws'
import { DocumentBuilder, SwaggerModule } from '@nestjs/swagger'

async function bootstrap() {
    const app = await NestFactory.create<NestExpressApplication>(AppModule, {
        logger: ['error', 'warn', 'log', 'debug', 'verbose']
    })
    app.useWebSocketAdapter(new WsAdapter(app))
    app.use(helmet())
    app.use(cookieParser())
    app.enableCors({
        origin: true,
        credentials: true
    })

    const config = new DocumentBuilder()
        .setTitle('Server')
        .setDescription('The server API description')
        .setVersion('1.0.0')
        .build()
    const document = SwaggerModule.createDocument(app, config)
    SwaggerModule.setup('api', app, document)

    await app.listen(3001)
}
bootstrap()
