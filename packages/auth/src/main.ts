import { NestFactory } from '@nestjs/core'
import { AuthModule } from './auth.module'
import { MicroserviceOptions, Transport } from '@nestjs/microservices'
import { ConfigService } from '@nestjs/config'
import {
    defaultAuthServicePort,
    kAuthServicePort
} from '@server-octopus/shared'

async function bootstrap() {
    const app = await NestFactory.create(AuthModule)
    const configService = app.get(ConfigService)
    const port =
        configService.get<number>(kAuthServicePort) ?? defaultAuthServicePort

    const microservice = app.connectMicroservice<MicroserviceOptions>({
        transport: Transport.TCP,
        options: {
            port
        }
    })
    await microservice.listen()
}
bootstrap()
